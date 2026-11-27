use std::{
    fs::File,
    io::Read,
    path::{Component, Path},
};

use assetcraft::Format;
use cap_primitives::fs::{self, OpenOptions, OpenOptionsExt};
use craft::behavior::kurbo::PathEl;
use craft::browse::thumbs::ThumbError;
use craft::host::queue::Cancel;
use craft::image_codecs::{ChannelLayout, Image};
use craft::mesh::{self, Limits, Scene};
use craft::svg::tree::draw::tiny_skia;

use crate::desktop_documents::StillPreview;

const MAX_EDGE: u32 = 768;
const MAX_VERTICES: usize = 500_000;
const MAX_TRIANGLES: usize = 300_000;
const MAX_DRAW_WORK: usize = 48_000_000;
const MAX_DEPENDENCY_BYTES: usize = 32 << 20;

pub fn render(
    path: &Path,
    format: Format,
    bytes: &[u8],
    edge: u32,
    cancel: &Cancel,
) -> StillPreview {
    let edge = edge.clamp(64, MAX_EDGE);
    let result = if format.is_cad() {
        cad(format, bytes, edge, cancel)
    } else {
        model(path, format, bytes, edge, cancel)
    };
    if cancel.is_cancelled() {
        return StillPreview::new(Err(ThumbError::Cancelled));
    }
    result.unwrap_or_else(|error| error_preview(format, bytes, error))
}

fn failed(error: impl std::fmt::Display) -> ThumbError {
    ThumbError::Failed(error.to_string())
}

fn error_preview(format: Format, bytes: &[u8], error: ThumbError) -> StillPreview {
    let description = match &error {
        ThumbError::Failed(message) => Some(message.clone()),
        _ => None,
    };
    let mut preview = StillPreview::new(Err(error));
    if let Some(description) = description {
        preview.properties.push(("Preview", description));
    }
    if format == Format::Dwg
        && let Some(version) = craft::dwg::version(bytes)
    {
        preview.properties.push(("DWG version", version));
    }
    preview
}

fn cancellation(cancel: &Cancel) -> Result<(), ThumbError> {
    if cancel.is_cancelled() {
        Err(ThumbError::Cancelled)
    } else {
        Ok(())
    }
}

fn sibling(root: &File, name: &str, remaining: &mut usize, cancel: &Cancel) -> Option<Vec<u8>> {
    if cancel.is_cancelled() || name.contains([':', '\\', '\0']) {
        return None;
    }
    let relative = Path::new(name);
    if relative
        .components()
        .any(|part| !matches!(part, Component::Normal(_) | Component::CurDir))
    {
        return None;
    }
    let mut options = OpenOptions::new();
    options.read(true);
    #[cfg(target_os = "linux")]
    options.custom_flags(0o4000);
    #[cfg(target_os = "macos")]
    options.custom_flags(0x0004);
    #[cfg(windows)]
    options.share_mode(0x0000_0001);
    let file = fs::open(root, relative, &options).ok()?;
    let stat = file.metadata().ok()?;
    if !stat.is_file() || stat.len() > *remaining as u64 {
        return None;
    }
    let mut bytes = Vec::new();
    file.take(*remaining as u64 + 1)
        .read_to_end(&mut bytes)
        .ok()?;
    *remaining = remaining.checked_sub(bytes.len())?;
    Some(bytes)
}

fn model(
    path: &Path,
    format: Format,
    bytes: &[u8],
    edge: u32,
    cancel: &Cancel,
) -> Result<StillPreview, ThumbError> {
    let model = load_model(path, format, bytes, cancel)?;
    let Model {
        scene,
        units,
        axis,
        z_up,
        instances,
    } = model;
    let vertices: usize = scene.meshes.iter().map(|mesh| mesh.positions.len()).sum();
    let triangles: usize = scene.meshes.iter().map(mesh::Mesh::triangle_count).sum();
    let mut preview = StillPreview::new(Err(ThumbError::Unsupported));
    for (name, count) in [
        ("Meshes", scene.meshes.len()),
        ("Vertices", vertices),
        ("Triangles", triangles),
        ("Instances", instances.len()),
    ] {
        preview.properties.push((name, count.to_string()));
    }
    if let Some(units) = units {
        preview.properties.push(("Model units", units));
    }
    if !scene.external.is_empty() {
        preview
            .properties
            .push(("Linked resources", scene.external.len().to_string()));
    }
    if !scene.warnings.is_empty() {
        preview
            .properties
            .push(("Model warnings", scene.warnings.join(" · ")));
    }
    preview.image = match project_scene(&scene, &instances, z_up, &axis, cancel) {
        Ok(mut geometry) => {
            if let Some([x, y, z]) = geometry.world.extent() {
                preview
                    .properties
                    .push(("Model size", format!("{x:.4} × {y:.4} × {z:.4}")));
            }
            raster_model(&scene, &mut geometry, edge, cancel)
        }
        Err(error) => Err(error),
    };
    record_preview_error(&mut preview);
    Ok(preview)
}

pub(crate) struct Model {
    pub(crate) scene: Scene,
    units: Option<String>,
    pub(crate) axis: mesh::Mat4,
    pub(crate) z_up: bool,
    pub(crate) instances: Vec<(usize, mesh::Mat4)>,
}

pub(crate) fn load_model(
    path: &Path,
    format: Format,
    bytes: &[u8],
    cancel: &Cancel,
) -> Result<Model, ThumbError> {
    let limits = Limits {
        max_vertices: MAX_VERTICES,
        max_triangles: MAX_TRIANGLES,
        max_face_corners: 256,
        max_meshes: 4096,
        max_parts: 8192,
        max_materials: 4096,
        max_nodes: 8192,
        max_data_bytes: 64 << 20,
        max_extras: 1024,
        ..Limits::default()
    };
    let root = path
        .parent()
        .and_then(|path| fs::open_ambient_dir(path, cap_primitives::ambient_authority()).ok());
    let mut bytes_left = MAX_DEPENDENCY_BYTES;
    let mut files_left = 64usize;
    let resolve = &mut |name: &str| {
        files_left = files_left.checked_sub(1)?;
        sibling(root.as_ref()?, name, &mut bytes_left, cancel)
    };
    let mut units = None;
    let mut axis = mesh::IDENTITY;
    let scene = match format {
        Format::Glb | Format::Gltf => {
            units = Some("Metres".to_owned());
            mesh::gltf::read(bytes, &limits, resolve)
        }
        Format::Obj => mesh::obj::read(bytes, &limits, resolve),
        Format::Stl => mesh::stl::read(bytes, &limits),
        Format::Ply => mesh::ply::read(bytes, &limits),
        Format::ThreeMf => mesh::threemf::read_model(bytes, &limits).map(|model| {
            units = Some(format!("{:?}", model.unit));
            model.scene
        }),
        Format::Fbx => mesh::fbx::read(bytes, &limits, &mut mesh::no_external),
        Format::Dae => mesh::dae::read_document(bytes, &limits).map(|document| {
            axis = document.up_axis.to_y_up();
            units = Some(format!("{} m per model unit", document.meter));
            document.scene
        }),
        Format::Abc => mesh::abc::read(bytes, &limits),
        Format::Usd => {
            let (scene, up_axis, meters) = usd_scene(bytes, &limits, cancel)?;
            axis = match up_axis.as_deref() {
                Some("Z") => mesh::dae::UpAxis::ZUp.to_y_up(),
                Some("X") => mesh::dae::UpAxis::XUp.to_y_up(),
                _ => mesh::IDENTITY,
            };
            units = meters.map(|meters| format!("{meters} m per model unit"));
            Ok(scene)
        }
        _ => return Err(ThumbError::Unsupported),
    }
    .map_err(failed)?;
    cancellation(cancel)?;
    let instances = scene.instances(&limits).map_err(failed)?;
    let z_up = matches!(format, Format::Stl | Format::Ply | Format::ThreeMf);
    Ok(Model {
        scene,
        units,
        axis,
        z_up,
        instances,
    })
}

fn usd_scene(
    bytes: &[u8],
    limits: &Limits,
    cancel: &Cancel,
) -> Result<(Scene, Option<String>, Option<f64>), ThumbError> {
    use craft::usd::scene::{Budget, PrimSpec, Val, read_layer_with_budget};
    let layer = read_layer_with_budget(bytes, &mut Budget::with_limit(64 << 20)).map_err(failed)?;
    let numbers = |prim: &PrimSpec, name: &str| -> Option<Vec<f64>> {
        match prim.props.get(name)?.value.as_ref()? {
            Val::Num(values) => Some(values.clone()),
            _ => None,
        }
    };
    let mut meshes = Vec::new();
    let mut vertices_left = limits.max_vertices;
    let mut triangles_left = limits.max_triangles;
    let mut skipped = 0;
    let mut pending = vec![(0, [0.0; 3], [1.0; 3])];
    let mut budget = limits.max_nodes;
    while let Some((index, offset, scale)) = pending.pop() {
        cancellation(cancel)?;
        budget = budget
            .checked_sub(1)
            .ok_or_else(|| failed("USD exceeds the prim preview budget"))?;
        let Some(prim) = layer.prims.get(index) else {
            continue;
        };
        if prim.active == Some(false) {
            continue;
        }
        let translate = numbers(prim, "xformOp:translate")
            .and_then(|v| <[f64; 3]>::try_from(v).ok())
            .unwrap_or([0.0; 3]);
        let factor = numbers(prim, "xformOp:scale")
            .and_then(|v| <[f64; 3]>::try_from(v).ok())
            .unwrap_or([1.0; 3]);
        let offset = std::array::from_fn::<_, 3, _>(|i| offset[i] + translate[i] * scale[i]);
        let scale = std::array::from_fn::<_, 3, _>(|i| factor[i] * scale[i]);
        skipped += usize::from(
            !prim.references.is_empty()
                || prim.props.keys().any(|key| {
                    key.starts_with("xformOp:")
                        && !matches!(key.as_str(), "xformOp:translate" | "xformOp:scale")
                }),
        );
        if prim.children.len() > budget.saturating_sub(pending.len()) {
            return Err(failed("USD exceeds the prim preview budget"));
        }
        pending.extend(
            prim.children
                .iter()
                .rev()
                .map(|&child| (child, offset, scale)),
        );
        if prim.type_name != "Mesh" {
            continue;
        }
        let (Some(points), Some(counts), Some(indices)) = (
            numbers(prim, "points"),
            numbers(prim, "faceVertexCounts"),
            numbers(prim, "faceVertexIndices"),
        ) else {
            skipped += 1;
            continue;
        };
        if !points.len().is_multiple_of(3)
            || meshes.len() >= limits.max_meshes
            || indices.len() > triangles_left.saturating_mul(3)
        {
            return Err(failed("USD exceeds the mesh preview budget"));
        }
        let sizes = counts
            .into_iter()
            .map(|count| {
                (count.fract() == 0.0 && count >= 3.0 && count <= limits.max_face_corners as f64)
                    .then_some(count as u32)
                    .ok_or_else(|| failed("Unsupported USD face size"))
            })
            .collect::<Result<Vec<_>, _>>()?;
        if sizes.iter().map(|&n| n as usize).sum::<usize>() != indices.len() {
            return Err(failed("USD face counts do not match the index array"));
        }
        vertices_left = vertices_left
            .checked_sub(points.len() / 3)
            .ok_or_else(|| failed("USD exceeds the vertex preview budget"))?;
        triangles_left = triangles_left
            .checked_sub(sizes.iter().map(|&n| n as usize - 2).sum())
            .ok_or_else(|| failed("USD exceeds the triangle preview budget"))?;
        let count = points.len() / 3;
        let corners = indices
            .into_iter()
            .map(|index| {
                (index.fract() == 0.0 && index >= 0.0 && index < count as f64)
                    .then_some(mesh::Corner::at(index as u32))
                    .ok_or_else(|| failed("USD face references an invalid vertex"))
            })
            .collect::<Result<Vec<_>, _>>()?;
        meshes.push(mesh::Mesh {
            name: prim.name.clone(),
            positions: points
                .as_chunks::<3>()
                .0
                .iter()
                .map(|point| std::array::from_fn(|i| point[i] * scale[i] + offset[i]))
                .collect(),
            parts: vec![mesh::Part {
                face_sizes: sizes,
                corners,
                ..Default::default()
            }],
            ..Default::default()
        });
    }
    if meshes.is_empty() {
        return Err(failed("This USD layer has no complete local static mesh"));
    }
    let mut scene = Scene::from_meshes(meshes, Vec::new());
    scene.warnings = layer.warnings;
    if skipped != 0 {
        scene
            .warnings
            .push("Some USD geometry, references or transforms were omitted".into());
    }
    Ok((scene, layer.up_axis, layer.meters_per_unit))
}

#[derive(Clone, Copy)]
struct Bounds {
    min: [f64; 3],
    max: [f64; 3],
}

impl Bounds {
    fn new() -> Self {
        Self {
            min: [f64::INFINITY; 3],
            max: [f64::NEG_INFINITY; 3],
        }
    }

    fn include(&mut self, point: [f64; 3]) -> Result<(), ThumbError> {
        if !point.into_iter().all(f64::is_finite) {
            return Err(failed("The model contains non-finite coordinates"));
        }
        for (axis, coordinate) in point.into_iter().enumerate() {
            self.min[axis] = self.min[axis].min(coordinate);
            self.max[axis] = self.max[axis].max(coordinate);
        }
        Ok(())
    }

    fn extent(self) -> Option<[f64; 3]> {
        let extent = std::array::from_fn(|axis| self.max[axis] - self.min[axis]);
        extent
            .iter()
            .all(|size| size.is_finite() && *size >= 0.0)
            .then_some(extent)
    }
}

struct Geometry {
    world: Bounds,
    view: Bounds,
    instances: Vec<(usize, Vec<[f64; 3]>)>,
}

fn project_scene(
    scene: &Scene,
    instances: &[(usize, mesh::Mat4)],
    z_up: bool,
    axis: &mesh::Mat4,
    cancel: &Cancel,
) -> Result<Geometry, ThumbError> {
    let mut geometry = Geometry {
        world: Bounds::new(),
        view: Bounds::new(),
        instances: Vec::with_capacity(instances.len()),
    };
    let mut vertices_left = MAX_VERTICES;
    let mut faces_left = MAX_TRIANGLES;
    for &(index, matrix) in instances {
        cancellation(cancel)?;
        let mesh = &scene.meshes[index];
        vertices_left = vertices_left
            .checked_sub(mesh.positions.len())
            .ok_or_else(|| failed("Instanced vertices exceed the preview limit"))?;
        faces_left = faces_left
            .checked_sub(mesh.triangle_count())
            .ok_or_else(|| failed("Instanced triangles exceed the preview limit"))?;
        let points = mesh
            .positions
            .iter()
            .map(|&position| {
                let world = mesh::transform_point(&matrix, position);
                geometry.world.include(world)?;
                let [x, y, z] = mesh::transform_point(axis, world);
                let (right, up, forward) = if z_up { (x, z, -y) } else { (x, y, z) };
                // Orthographic three-quarter view with a Y-up or Z-up file convention.
                let across = std::f64::consts::FRAC_1_SQRT_2 * (right - forward);
                let toward = std::f64::consts::FRAC_1_SQRT_2 * (right + forward);
                let view = [
                    across,
                    up * 0.866_025_403_784_438_6 - toward * 0.5,
                    up * 0.5 + toward * 0.866_025_403_784_438_6,
                ];
                geometry.view.include(view)?;
                Ok(view)
            })
            .collect::<Result<Vec<_>, ThumbError>>()?;
        geometry.instances.push((index, points));
    }
    Ok(geometry)
}

pub(crate) struct Poster {
    edge: usize,
    pub(crate) color: Vec<u8>,
    depth: Vec<f64>,
    work_left: usize,
}

impl Poster {
    pub(crate) fn new(edge: u32) -> Self {
        Self::with_work_budget(edge, MAX_DRAW_WORK, [0; 4])
    }

    pub(crate) fn with_work_budget(edge: u32, work_left: usize, background: [u8; 4]) -> Self {
        let edge = edge as usize;
        Self {
            edge,
            color: background.repeat(edge * edge),
            depth: vec![f64::NEG_INFINITY; edge * edge],
            work_left,
        }
    }

    pub(crate) fn pixel(&mut self, x: usize, y: usize, depth: f64, rgb: [u8; 3]) {
        let i = y * self.edge + x;
        if self.depth[i] < depth {
            self.depth[i] = depth;
            self.color[i * 4..i * 4 + 3].copy_from_slice(&rgb);
            self.color[i * 4 + 3] = 255;
        }
    }

    pub(crate) fn covered(&self, x: usize, y: usize) -> bool {
        self.depth[y * self.edge + x].is_finite()
    }

    pub(crate) fn triangle(
        &mut self,
        [a, b, c]: [[f64; 3]; 3],
        color: impl FnOnce() -> [u8; 3],
        wire: bool,
    ) -> Result<(), ThumbError> {
        let signed_area = |a: [f64; 3], b: [f64; 3], x: f64, y: f64| {
            (b[0] - a[0]) * (y - a[1]) - (b[1] - a[1]) * (x - a[0])
        };
        let area = signed_area(a, b, c[0], c[1]);
        if area.abs() < 1e-9 {
            return Ok(());
        }
        let last = (self.edge - 1) as f64;
        let range = |axis| {
            let low = [a, b, c]
                .map(|p: [f64; 3]| p[axis])
                .into_iter()
                .fold(f64::INFINITY, f64::min);
            let high = [a, b, c]
                .map(|p: [f64; 3]| p[axis])
                .into_iter()
                .fold(f64::NEG_INFINITY, f64::max);
            let start = (low - 0.5).ceil().max(0.0);
            let end = (high - 0.5).floor().min(last);
            (start <= end).then_some((start as usize, end as usize))
        };
        let (Some((x0, x1)), Some((y0, y1))) = (range(0), range(1)) else {
            return Ok(());
        };
        self.work_left = self
            .work_left
            .checked_sub((x1 - x0 + 1) * (y1 - y0 + 1))
            .ok_or_else(|| failed("Model rasterization exceeds the preview work limit"))?;
        let color = color();
        let edge_thresholds = if wire {
            [(b, c), (c, a), (a, b)]
                .map(|(start, end)| (end[0] - start[0]).hypot(end[1] - start[1]) / area.abs())
        } else {
            [0.0; 3]
        };
        let step_u = (b[1] - c[1]) / area;
        let step_v = (c[1] - a[1]) / area;
        let first_x = x0 as f64 + 0.5;
        for y in y0..=y1 {
            let py = y as f64 + 0.5;
            let mut u = signed_area(b, c, first_x, py) / area;
            let mut v = signed_area(c, a, first_x, py) / area;
            for x in x0..=x1 {
                let w = 1.0 - u - v;
                if u >= 0.0 && v >= 0.0 && w >= 0.0 {
                    let outline = wire
                        && [u, v, w]
                            .into_iter()
                            .zip(edge_thresholds)
                            .any(|(weight, threshold)| weight <= threshold);
                    let pixel = if outline { [32, 42, 55] } else { color };
                    self.pixel(x, y, u * a[2] + v * b[2] + w * c[2], pixel);
                }
                u += step_u;
                v += step_v;
            }
        }
        Ok(())
    }
}

fn raster_model(
    scene: &Scene,
    geometry: &mut Geometry,
    edge: u32,
    cancel: &Cancel,
) -> Result<Image, ThumbError> {
    let [w, h, d] = geometry.view.extent().ok_or(ThumbError::Unsupported)?;
    let span = w.max(h).max(d);
    let scale = f64::from(edge) * 0.83;
    let center = [w, h, d].map(|span| span * 0.5);
    let middle = f64::from(edge) * 0.5;
    let mut poster = Poster::new(edge);
    for (index, positions) in &mut geometry.instances {
        cancellation(cancel)?;
        let mesh = &scene.meshes[*index];
        for point in positions.iter_mut() {
            for axis in 0..3 {
                point[axis] = if span > 0.0 {
                    (((point[axis] - geometry.view.min[axis]) - center[axis]) / span) * scale
                } else {
                    0.0
                };
            }
            point[0] += middle;
            point[1] = middle - point[1];
        }
        if mesh.parts.is_empty() {
            for &[x, y, depth] in positions.iter() {
                poster.pixel(x as usize, y as usize, depth, [44, 94, 146]);
            }
        }
        for part in &mesh.parts {
            let material = part.material.and_then(|index| scene.materials.get(index));
            let base = material.map_or([0.16, 0.37, 0.58, 1.0], |material| material.base_color);
            for (i, corners) in part.triangles_in(&mesh.positions).enumerate() {
                if i.is_multiple_of(512) {
                    cancellation(cancel)?;
                }
                let [Some(a), Some(b), Some(c)] =
                    corners.map(|corner| positions.get(corner.position as usize))
                else {
                    continue;
                };
                let normal = mesh::face_normal([*a, *b, *c]).unwrap_or([0.0, 0.0, 0.0]);
                let lighting =
                    0.35 + 0.65 * (0.3 * normal[0] - 0.4 * normal[1] + 0.866 * normal[2]).abs();
                let rgb = [base[0], base[1], base[2]]
                    .map(|value| craft::color::linear_to_srgb_u8(value * lighting as f32));
                poster.triangle([*a, *b, *c], || rgb, false)?;
            }
        }
    }
    if poster.depth.iter().all(|depth| !depth.is_finite()) {
        return Err(ThumbError::Unsupported);
    }
    rgba(edge, poster.color)
}

fn cad(
    format: Format,
    bytes: &[u8],
    edge: u32,
    cancel: &Cancel,
) -> Result<StillPreview, ThumbError> {
    let converted;
    let mut notes = Vec::new();
    let dxf = if format == Format::Dwg {
        let (payload, warnings) = craft::dwg::decode(
            bytes,
            craft::dwg::Limits {
                max_dwg_bytes: 64 << 20,
                max_dxf_bytes: 64 << 20,
            },
        )
        .map_err(failed)?;
        converted = payload;
        notes = warnings;
        converted.as_slice()
    } else {
        bytes
    };
    cancellation(cancel)?;
    let drawing = craft::dxf::paths::import(dxf).map_err(failed)?;
    let mut preview = StillPreview::new(Err(ThumbError::Unsupported));
    if format == Format::Dwg
        && let Some(version) = craft::dwg::version(bytes)
    {
        preview.properties.push(("DWG version", version));
    }
    preview
        .properties
        .push(("Drawable paths", drawing.paths.len().to_string()));
    if let Some(bounds) = drawing.bounds {
        preview.properties.push((
            "Drawing size",
            format!("{:.4} × {:.4}", bounds.width(), bounds.height()),
        ));
        preview.properties.push((
            "Millimetres per drawing unit",
            drawing.unit_to_mm.to_string(),
        ));
        preview.image = draw_cad(&drawing, bounds, edge, cancel);
    }
    notes.extend(drawing.warnings);
    if !notes.is_empty() {
        preview
            .properties
            .push(("Drawing warnings", notes.join(" · ")));
    }
    record_preview_error(&mut preview);
    Ok(preview)
}

fn draw_cad(
    drawing: &craft::dxf::paths::Imported,
    bounds: craft::behavior::kurbo::Rect,
    edge: u32,
    cancel: &Cancel,
) -> Result<Image, ThumbError> {
    let span = bounds.width().max(bounds.height());
    if !span.is_finite() || span <= 0.0 {
        return Err(ThumbError::Unsupported);
    }
    let edge_f = f64::from(edge);
    let scale = edge_f * 0.84 / span;
    let offset_x = (edge_f - bounds.width() * scale) * 0.5;
    let offset_y = (edge_f - bounds.height() * scale) * 0.5;
    let point = |point: craft::behavior::kurbo::Point| {
        (
            ((point.x - bounds.x0) * scale + offset_x) as f32,
            ((bounds.y1 - point.y) * scale + offset_y) as f32,
        )
    };
    let mut pixmap = tiny_skia::Pixmap::new(edge, edge)
        .ok_or_else(|| failed("Drawing preview allocation failed"))?;
    pixmap.fill(tiny_skia::Color::WHITE);
    let mut paint = tiny_skia::Paint {
        anti_alias: true,
        ..Default::default()
    };
    paint.set_color_rgba8(37, 66, 92, 255);
    let stroke = tiny_skia::Stroke {
        width: (edge as f32 / 200.0).max(1.0),
        ..Default::default()
    };
    for (i, line) in drawing.paths.iter().enumerate() {
        if i.is_multiple_of(64) {
            cancellation(cancel)?;
        }
        let mut path = tiny_skia::PathBuilder::new();
        for element in line.elements() {
            match *element {
                PathEl::MoveTo(p) => {
                    let (x, y) = point(p);
                    path.move_to(x, y);
                }
                PathEl::LineTo(p) => {
                    let (x, y) = point(p);
                    path.line_to(x, y);
                }
                PathEl::QuadTo(a, b) => {
                    let (x, y) = point(a);
                    let (u, v) = point(b);
                    path.quad_to(x, y, u, v);
                }
                PathEl::CurveTo(a, b, c) => {
                    let (x, y) = point(a);
                    let (u, v) = point(b);
                    let (s, t) = point(c);
                    path.cubic_to(x, y, u, v, s, t);
                }
                PathEl::ClosePath => path.close(),
            }
        }
        if let Some(path) = path.finish() {
            pixmap.stroke_path(
                &path,
                &paint,
                &stroke,
                tiny_skia::Transform::identity(),
                None,
            );
        }
    }
    rgba(edge, pixmap.take())
}

fn record_preview_error(preview: &mut StillPreview) {
    if let Err(ThumbError::Failed(reason)) = &preview.image {
        preview.properties.push(("Preview", reason.clone()));
    }
}

fn rgba(edge: u32, bytes: Vec<u8>) -> Result<Image, ThumbError> {
    Image::from_u8(edge, edge, ChannelLayout::Rgba, bytes).map_err(failed)
}
