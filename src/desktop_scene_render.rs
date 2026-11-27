use std::{
    f64::consts::{PI, TAU},
    io::Read,
    path::Path,
};

use assetcraft::{Format, local_file};
use craft::browse::thumbs::ThumbError;
use craft::host::queue::Cancel;
use craft::image_codecs::{self, DecodeOptions, Image};
use craft::mesh;

use crate::desktop_documents::{failed, rgba};
use crate::desktop_models::{Poster, load_model};

const FILE_BUDGET: usize = 64 << 20;
const VERTEX_BUDGET: usize = 500_000;
const TRIANGLE_BUDGET: usize = 300_000;
const MAX_SAMPLE_EDGE: u32 = 3072;
// The normalized model and floor grid both fit in front of this eye position.
const EYE_DISTANCE: f64 = 2.5;
type Point = [f64; 3];

#[derive(Clone, Copy, Default, PartialEq, Eq)]
pub enum Projection {
    Orthographic,
    #[default]
    Perspective,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Quality {
    Interactive,
    Settled,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub struct Studio {
    pub background: [u8; 3],
    pub grid: [u8; 3],
    pub axis: [u8; 3],
}

impl Default for Studio {
    fn default() -> Self {
        Self {
            background: [242, 246, 250],
            grid: [169, 185, 202],
            axis: [102, 120, 143],
        }
    }
}

#[derive(Clone, Copy, PartialEq)]
pub struct Orbit {
    pub projection: Projection,
    pub studio: Studio,
    pub azimuth: f64,
    pub elevation: f64,
    pub zoom: f64,
    pub offset: [f64; 2],
    pub grid: bool,
    pub wire: bool,
    pub light_azimuth: f64,
    pub light_elevation: f64,
    pub light_strength: f64,
    pub environment_rotation: f64,
    pub exposure: f64,
    pub show_environment: bool,
}

impl Default for Orbit {
    fn default() -> Self {
        Self {
            projection: Projection::default(),
            studio: Studio::default(),
            azimuth: 45.0,
            elevation: 24.0,
            zoom: 1.0,
            offset: [0.0; 2],
            grid: true,
            wire: false,
            light_azimuth: 35.0,
            light_elevation: 55.0,
            light_strength: 1.0,
            environment_rotation: 0.0,
            exposure: 0.0,
            show_environment: true,
        }
    }
}

struct Face {
    indices: [u32; 3],
    normal: Point,
    color: [f32; 3],
}

#[derive(Default)]
pub struct Scene {
    points: Vec<Point>,
    faces: Vec<Face>,
    standalone: Vec<usize>,
    floor: f64,
}

impl Scene {
    pub fn open(path: &Path, format: Format, cancel: &Cancel) -> Result<Self, ThumbError> {
        let bytes = local_bytes(path)?;
        let model = load_model(path, format, &bytes, cancel)?;
        let mut result = Self::default();
        let mut min = [f64::INFINITY; 3];
        let mut max = [f64::NEG_INFINITY; 3];
        for (index, matrix) in model.instances {
            cancelled(cancel)?;
            let geometry = &model.scene.meshes[index];
            let triangle_count = geometry.triangle_count();
            if geometry.positions.len() > VERTEX_BUDGET.saturating_sub(result.points.len())
                || triangle_count > TRIANGLE_BUDGET.saturating_sub(result.faces.len())
            {
                return Err(failed("Instanced geometry exceeds the 3D viewer budget"));
            }
            result.points.reserve(geometry.positions.len());
            result.faces.reserve(triangle_count);
            let transform = mesh::mat_mul(&model.axis, &matrix);
            let identity = transform == mesh::IDENTITY;
            let first = result.points.len() as u32;
            for (i, &point) in geometry.positions.iter().enumerate() {
                if i.is_multiple_of(4096) {
                    cancelled(cancel)?;
                }
                let [x, y, z] = if identity {
                    point
                } else {
                    mesh::transform_point(&transform, point)
                };
                let coords = if model.z_up { [x, z, -y] } else { [x, y, z] };
                if !coords.into_iter().all(f64::is_finite) {
                    return Err(failed("The model contains non-finite coordinates"));
                }
                for i in 0..3 {
                    min[i] = min[i].min(coords[i]);
                    max[i] = max[i].max(coords[i]);
                }
                result.points.push(coords);
            }
            if geometry.parts.is_empty() {
                result
                    .standalone
                    .extend(first as usize..result.points.len());
            }
            for part in &geometry.parts {
                let [r, g, b, _] = part
                    .material
                    .and_then(|i| model.scene.materials.get(i))
                    .map_or([0.21, 0.36, 0.54, 1.0], |material| material.base_color);
                let mut push = |i: usize, face: [mesh::Corner; 3]| -> Result<(), ThumbError> {
                    if i.is_multiple_of(512) {
                        cancelled(cancel)?;
                    }
                    let indices = face.map(|c| c.position);
                    if indices
                        .iter()
                        .any(|&i| i as usize >= geometry.positions.len())
                    {
                        return Ok(());
                    }
                    let indices = indices.map(|i| first + i);
                    let points = indices.map(|i| result.points[i as usize]);
                    if let Some(normal) = mesh::face_normal(points) {
                        result.faces.push(Face {
                            indices,
                            normal,
                            color: [r, g, b],
                        });
                    }
                    Ok(())
                };
                if part.face_sizes.is_empty() {
                    for (i, &face) in part.corners.as_chunks::<3>().0.iter().enumerate() {
                        push(i, face)?;
                    }
                } else {
                    for (i, face) in part.triangles_in(&geometry.positions).enumerate() {
                        push(i, face)?;
                    }
                }
            }
        }
        if result.points.is_empty() || (result.faces.is_empty() && result.standalone.is_empty()) {
            return Err(ThumbError::Unsupported);
        }
        let span = (0..3).map(|i| max[i] - min[i]).fold(0.0, f64::max);
        if !span.is_finite() {
            return Err(failed("The model bounds exceed the preview range"));
        }
        if span > 0.0 {
            let center = std::array::from_fn::<_, 3, _>(|i| (max[i] - min[i]) * 0.5);
            for point in &mut result.points {
                for i in 0..3 {
                    point[i] = ((point[i] - min[i]) - center[i]) / span;
                }
            }
            result.floor = -center[1] / span;
        } else {
            result.points.fill([0.0; 3]);
        }
        Ok(result)
    }

    pub fn render(
        &self,
        view: Orbit,
        environment: Option<&Environment>,
        edge: u32,
        quality: Quality,
        cancel: &Cancel,
    ) -> Result<Image, ThumbError> {
        let edge = edge.clamp(96, 2048);
        let work_budget = |size: u32| (size as usize).pow(2).saturating_mul(32).max(48_000_000);
        if quality == Quality::Settled {
            let samples = (edge * 2).min(MAX_SAMPLE_EDGE);
            match self.draw(view, environment, samples, work_budget(edge), cancel) {
                Ok(pixels) => return rgba(edge, edge, resolve(&pixels, samples, edge, cancel)?),
                Err(ThumbError::Failed(_)) => {}
                Err(error) => return Err(error),
            }
        }
        let mut last_size = None;
        for size in [edge, edge * 3 / 4, edge / 2, edge / 4].map(|size| size.max(96)) {
            if last_size.replace(size) == Some(size) {
                continue;
            }
            match self.draw(view, environment, size, work_budget(size), cancel) {
                Err(ThumbError::Failed(_)) => continue,
                output => return rgba(size, size, output?),
            }
        }
        Err(failed("Visible geometry exceeds the 3D rendering budget"))
    }

    fn draw(
        &self,
        view: Orbit,
        environment: Option<&Environment>,
        edge: u32,
        work: usize,
        cancel: &Cancel,
    ) -> Result<Vec<u8>, ThumbError> {
        cancelled(cancel)?;
        let camera = Camera::new(view, edge);
        let [r, g, b] = view.studio.background;
        let mut poster = Poster::with_work_budget(edge, work, [r, g, b, 255]);
        if view.grid {
            let studio = if environment.is_some() && view.show_environment {
                Studio::default()
            } else {
                view.studio
            };
            let y = self.floor - 0.003;
            let limit = 5;
            for i in -limit..=limit {
                let line = f64::from(i) * 0.25;
                let end = f64::from(limit) * 0.25;
                let color = if i == 0 { studio.axis } else { studio.grid };
                draw_grid_line(
                    &mut poster,
                    &camera,
                    [[-end, y, line], [end, y, line]],
                    color,
                    edge,
                );
                draw_grid_line(
                    &mut poster,
                    &camera,
                    [[line, y, -end], [line, y, end]],
                    color,
                    edge,
                );
            }
        }
        let projected: Vec<_> = self.points.iter().map(|&p| camera.project(p)).collect();
        for &i in &self.standalone {
            let [x, y, z] = projected[i];
            if x >= 0.0 && y >= 0.0 && x < f64::from(edge) && y < f64::from(edge) {
                poster.pixel(x as usize, y as usize, z, [32, 87, 143]);
            }
        }
        let light = direction(view.light_azimuth, view.light_elevation);
        let rotation = -view.environment_rotation.to_radians();
        let exposure = 2f32.powf(view.exposure as f32);
        for (i, face) in self.faces.iter().enumerate() {
            if i.is_multiple_of(256) {
                cancelled(cancel)?;
            }
            let vertices = face.indices.map(|i| projected[i as usize]);
            let color = || {
                let norm = if camera.faces_away(face.normal, self.points[face.indices[0] as usize])
                {
                    face.normal.map(|component| -component)
                } else {
                    face.normal
                };
                let key = (dot(norm, light).max(0.0) * view.light_strength) as f32;
                let ambient = environment.map_or([0.38; 3], |env| {
                    env.diffuse(rotate_y(norm, rotation))
                        .map(|ambient| (ambient * exposure).min(8.0) + 0.16)
                });
                tone(std::array::from_fn(|i| {
                    (face.color[i] * (ambient[i] + key)).max(0.0)
                }))
            };
            poster.triangle(vertices, color, view.wire)?;
        }
        if view.show_environment
            && let Some(environment) = environment
        {
            environment.background(&mut poster, &camera, edge, view, cancel)?;
        }
        Ok(poster.color)
    }
}

/// Box-filter the sample grid. Integer overlap weights also cover the fractional ratio
/// used when the sample buffer reaches its size limit.
fn resolve(
    source: &[u8],
    sample_edge: u32,
    edge: u32,
    cancel: &Cancel,
) -> Result<Vec<u8>, ThumbError> {
    let (sample_edge, edge) = (sample_edge as usize, edge as usize);
    let mut pixels = Vec::with_capacity(edge * edge * 4);
    if sample_edge == edge * 2 {
        let stride = sample_edge * 4;
        for pair in source.chunks_exact(stride * 2) {
            cancelled(cancel)?;
            let (top, bottom) = pair.split_at(stride);
            for (top, bottom) in top.as_chunks::<8>().0.iter().zip(bottom.as_chunks::<8>().0) {
                let [r, g, b] = [0, 1, 2].map(|i| {
                    let sum = u16::from(top[i])
                        + u16::from(top[i + 4])
                        + u16::from(bottom[i])
                        + u16::from(bottom[i + 4]);
                    ((sum + 2) / 4) as u8
                });
                pixels.extend_from_slice(&[r, g, b, 255]);
            }
        }
        return Ok(pixels);
    }
    let spans: Vec<_> = (0..edge)
        .map(|index| {
            let start = index * sample_edge;
            let end = start + sample_edge;
            (start / edge..end.div_ceil(edge))
                .map(|sample| {
                    let overlap = ((sample + 1) * edge).min(end) - (sample * edge).max(start);
                    (sample, overlap as u32)
                })
                .collect::<Vec<_>>()
        })
        .collect();
    let weight = (sample_edge * sample_edge) as u32;
    for rows in &spans {
        cancelled(cancel)?;
        for columns in &spans {
            let mut sum = [0u32; 3];
            for &(sy, wy) in rows {
                for &(sx, wx) in columns {
                    let offset = (sy * sample_edge + sx) * 4;
                    for (channel, value) in sum.iter_mut().zip(&source[offset..offset + 3]) {
                        *channel += u32::from(*value) * wx * wy;
                    }
                }
            }
            let [r, g, b] = sum.map(|channel| ((channel + weight / 2) / weight) as u8);
            pixels.extend_from_slice(&[r, g, b, 255]);
        }
    }
    Ok(pixels)
}

fn cancelled(cancel: &Cancel) -> Result<(), ThumbError> {
    if cancel.is_cancelled() {
        Err(ThumbError::Cancelled)
    } else {
        Ok(())
    }
}

fn local_bytes(path: &Path) -> Result<Vec<u8>, ThumbError> {
    let file = local_file::open_read(path).map_err(|error| {
        if error.kind() == std::io::ErrorKind::NotFound {
            ThumbError::Missing
        } else {
            failed(error)
        }
    })?;
    let metadata = file.metadata().map_err(failed)?;
    if !metadata.is_file() {
        return Err(ThumbError::Missing);
    }
    if metadata.len() > FILE_BUDGET as u64 {
        return Err(failed("Original exceeds the 3D viewer read limit"));
    }
    let mut bytes = Vec::new();
    file.take(FILE_BUDGET as u64 + 1)
        .read_to_end(&mut bytes)
        .map_err(failed)?;
    if bytes.len() > FILE_BUDGET {
        return Err(failed("Original exceeds the 3D viewer read limit"));
    }
    Ok(bytes)
}

struct Camera {
    projection: Projection,
    right: Point,
    up: Point,
    forward: Point,
    scale: f64,
    center: [f64; 2],
}

fn dot(a: Point, b: Point) -> f64 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

fn direction(yaw: f64, pitch: f64) -> Point {
    let (sy, cy) = yaw.to_radians().sin_cos();
    let (sp, cp) = pitch.to_radians().sin_cos();
    [sy * cp, sp, cy * cp]
}

impl Camera {
    fn new(view: Orbit, edge: u32) -> Self {
        let (sy, cy) = view.azimuth.to_radians().sin_cos();
        let (sp, cp) = view.elevation.to_radians().sin_cos();
        Self {
            projection: view.projection,
            right: [cy, 0.0, -sy],
            up: [-sy * sp, cp, -cy * sp],
            forward: direction(view.azimuth, view.elevation),
            scale: 0.55 * f64::from(edge) * view.zoom,
            center: view.offset.map(|offset| (0.5 + offset) * f64::from(edge)),
        }
    }

    fn project(&self, point: Point) -> Point {
        let depth = dot(point, self.forward);
        let (scale, depth) = match self.projection {
            Projection::Orthographic => (self.scale, depth),
            Projection::Perspective => {
                // Reciprocal depth interpolates linearly across screen-space triangles.
                let reciprocal = (EYE_DISTANCE - depth).recip();
                (self.scale * EYE_DISTANCE * reciprocal, reciprocal)
            }
        };
        [
            self.center[0] + dot(point, self.right) * scale,
            self.center[1] - dot(point, self.up) * scale,
            depth,
        ]
    }

    fn faces_away(&self, normal: Point, point: Point) -> bool {
        let facing = dot(normal, self.forward);
        match self.projection {
            Projection::Orthographic => facing < 0.0,
            Projection::Perspective => facing * EYE_DISTANCE < dot(normal, point),
        }
    }
}

fn draw_grid_line(
    poster: &mut Poster,
    camera: &Camera,
    points: [Point; 2],
    color: [u8; 3],
    edge: u32,
) {
    let [a, b] = points.map(|p| camera.project(p));
    let n = ((b[0] - a[0]).abs().max((b[1] - a[1]).abs()).ceil() as u32).clamp(1, edge * 16);
    for i in 0..=n {
        let t = f64::from(i) / f64::from(n);
        let [x, y, z] = std::array::from_fn(|i| a[i] + (b[i] - a[i]) * t);
        if x >= 0.0 && y >= 0.0 && x < f64::from(edge) && y < f64::from(edge) {
            poster.pixel(x as usize, y as usize, z, color);
        }
    }
}

const LIGHT_SAMPLES_W: usize = 384;
const LIGHT_SAMPLES_H: usize = LIGHT_SAMPLES_W / 2;
const LOBES: [Point; 6] = [
    [1., 0., 0.],
    [-1., 0., 0.],
    [0., 1., 0.],
    [0., -1., 0.],
    [0., 0., 1.],
    [0., 0., -1.],
];

pub struct Environment {
    pixels: Vec<[f32; 3]>,
    width: usize,
    height: usize,
    diffuse: [[f32; 3]; 6],
}

impl Environment {
    pub fn open(path: &Path, cancel: &Cancel) -> Result<Self, ThumbError> {
        let bytes = local_bytes(path)?;
        let limits = image_codecs::Limits {
            max_pixels: 18_000_000,
            max_alloc: 192 << 20,
            ..Default::default()
        };
        let image = image_codecs::decode_with(
            &bytes,
            &DecodeOptions {
                limits,
                ..Default::default()
            },
        )
        .map_err(failed)?;
        let (w, h) = image.dimensions();
        if !(1.9..=2.1).contains(&(f64::from(w) / f64::from(h))) {
            return Err(failed("Choose a 2:1 equirectangular HDR or EXR panorama"));
        }
        let channels = image.layout().channels();
        let gray = matches!(
            image.layout(),
            image_codecs::ChannelLayout::Gray | image_codecs::ChannelLayout::GrayA
        );
        let width = (w as usize).min(4096);
        let height = (h as usize * width / w as usize).max(1);
        let mut env = Self {
            pixels: Vec::with_capacity(width * height),
            width,
            height,
            diffuse: [[0.; 3]; 6],
        };
        for y in 0..height {
            cancelled(cancel)?;
            for x in 0..width {
                let i = (y * h as usize / height * w as usize + x * w as usize / width) * channels;
                let rgb = std::array::from_fn(|channel| {
                    let value = image.sample_normalized(i + if gray { 0 } else { channel });
                    if value.is_finite() {
                        value.clamp(0.0, 65_504.0)
                    } else {
                        0.0
                    }
                });
                env.pixels.push(rgb);
            }
        }
        let mut weights = [0.0; 6];
        for y in 0..LIGHT_SAMPLES_H {
            cancelled(cancel)?;
            let v = (y as f64 + 0.5) / LIGHT_SAMPLES_H as f64;
            let theta = PI * v;
            let solid_angle = theta.sin();
            for x in 0..LIGHT_SAMPLES_W {
                let u = (x as f64 + 0.5) / LIGHT_SAMPLES_W as f64;
                let rgb = env.sample(u, v);
                let phi = TAU * u - PI;
                let point = [
                    phi.sin() * solid_angle,
                    theta.cos(),
                    phi.cos() * solid_angle,
                ];
                for (k, lobe) in LOBES.into_iter().enumerate() {
                    let weight = (dot(point, lobe).max(0.0) * solid_angle) as f32;
                    weights[k] += weight;
                    for (sum, sample) in env.diffuse[k].iter_mut().zip(rgb) {
                        *sum += sample * weight;
                    }
                }
            }
        }
        for (lobe, weight) in env.diffuse.iter_mut().zip(weights) {
            *lobe = lobe.map(|value| value / weight.max(1.0));
        }
        Ok(env)
    }

    fn sample(&self, u: f64, v: f64) -> [f32; 3] {
        let x = (u * self.width as f64 - 0.5).rem_euclid(self.width as f64);
        let y = (v * self.height as f64 - 0.5).clamp(0.0, (self.height - 1) as f64);
        let x0 = x as usize % self.width;
        let x1 = (x0 + 1) % self.width;
        let y0 = y as usize;
        let y1 = (y0 + 1).min(self.height - 1);
        let [tx, ty] = [x.fract() as f32, y.fract() as f32];
        let pixel = |col, row: usize| self.pixels[row * self.width + col];
        let [a, b, c, d] = [pixel(x0, y0), pixel(x1, y0), pixel(x0, y1), pixel(x1, y1)];
        std::array::from_fn(|i| {
            (a[i] + (b[i] - a[i]) * tx) * (1.0 - ty) + (c[i] + (d[i] - c[i]) * tx) * ty
        })
    }

    fn diffuse(&self, direction: Point) -> [f32; 3] {
        let mut rgb = [0.0; 3];
        for (axis, coordinate) in direction.into_iter().enumerate() {
            let lobe = axis * 2 + usize::from(coordinate < 0.0);
            for (value, light) in rgb.iter_mut().zip(self.diffuse[lobe]) {
                *value += (coordinate * coordinate) as f32 * light;
            }
        }
        rgb
    }

    fn background(
        &self,
        poster: &mut Poster,
        camera: &Camera,
        edge: u32,
        view: Orbit,
        cancel: &Cancel,
    ) -> Result<(), ThumbError> {
        let angle = -view.environment_rotation.to_radians();
        let exposure = 2f32.powf(view.exposure as f32);
        let [right, up, forward] =
            [camera.right, camera.up, camera.forward].map(|p| rotate_y(p, angle));
        let ([cx, cy], focal) = match camera.projection {
            Projection::Orthographic => ([f64::from(edge) * 0.5; 2], f64::from(edge)),
            Projection::Perspective => (camera.center, camera.scale * EYE_DISTANCE),
        };
        let inverse_focal = focal.recip();
        for y in 0..edge {
            if y.is_multiple_of(16) {
                cancelled(cancel)?;
            }
            let dy = (cy - f64::from(y)) * inverse_focal;
            for x in 0..edge {
                if poster.covered(x as usize, y as usize) {
                    continue;
                }
                let dx = (f64::from(x) - cx) * inverse_focal;
                let dir =
                    std::array::from_fn::<_, 3, _>(|i| -forward[i] + right[i] * dx + up[i] * dy);
                let u = ((dir[0].atan2(dir[2]) + PI) / TAU).rem_euclid(1.0);
                let v = (dir[1] / dot(dir, dir).sqrt()).clamp(-1.0, 1.0).acos() / PI;
                let offset = ((y * edge + x) * 4) as usize;
                poster.color[offset..offset + 3]
                    .copy_from_slice(&tone(self.sample(u, v).map(|v| v * exposure)));
            }
        }
        Ok(())
    }
}

fn rotate_y([x, y, z]: Point, angle: f64) -> Point {
    let (sin, cos) = angle.sin_cos();
    [cos * x + sin * z, y, -sin * x + cos * z]
}

fn tone(rgb: [f32; 3]) -> [u8; 3] {
    let peak = rgb.into_iter().fold(0.0, f32::max);
    rgb.map(|value| craft::color::linear_to_srgb_u8(value / (1.0 + peak)))
}
