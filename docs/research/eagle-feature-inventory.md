# Eagle feature inventory

The approved comparison baseline is Eagle 4.0.0 Build 23, released on 1 April 2026 for Windows and macOS. The inventory draws on official public documentation consulted on 25 November 2026. [The decision registry](../decisions/REGISTRY.md) records the baseline under D04a and the approved MVP workflows under D04b. Implementation has yet to start. [Download and release](https://en.eagle.cool/download) · [Desktop support](https://en.eagle.cool/support/desktop)

The approved demonstration runs on Linux. It includes reopening a local library, batch import of PNG and JPEG with thumbnails and progress, organization and search, trash recovery, and original export. The minimal CLI covers library creation, import, search and export. The table records detailed acceptance proposals for those workflows. Details beyond D04b remain proposals.

| ID | MVP workflow | Proposed acceptance |
| --- | --- | --- |
| E-LIB-01 | Create and reopen a local library. [Libraries](https://en.eagle.cool/support/desktop/library) | Preserve IDs, metadata and associations after restart. Explain unsupported library versions and locks. |
| E-IMP-01 | Import local JPEG and PNG files. [Import](https://en.eagle.cool/support/article/importing-files) · [Duplicates](https://en.eagle.cool/support/article/how-to-handle-the-duplicate-image-notification-in-eagle) | Support batch progress, cancellation and per-file errors. Preserve original hashes and handle exact duplicates consistently. Recover without orphan records after interruption. |
| E-BRW-01 | Browse thumbnails and inspect an asset. [Browse](https://en.eagle.cool/support/desktop/browse) | Load thumbnails asynchronously and support mouse and keyboard selection. Show observed format, dimensions and decode errors. |
| E-ORG-01 | Organize assets in collections and apply tags, notes and ratings. [Organize](https://en.eagle.cool/support/desktop/organize) | Preserve multiple memberships, bulk tags and edited metadata after restart. |
| E-SRC-01 | Find assets by name and tags with a filter and sort order. [Search](https://en.eagle.cool/support/desktop/search) | Refresh results after mutations and reproduce them after rebuilding the index. |
| E-TRS-01 | Trash and restore assets. [Trash](https://en.eagle.cool/support/article/does-eagle-automatically-delete-items-in-the-trash) | Restore the same ID, original bytes, metadata and memberships. |
| E-OUT-01 | Export originals and open or reveal files. [Export](https://en.eagle.cool/support/article/export-to-computer) | Match source hashes and preserve files already present at the destination. Report per-file errors. |

## Proposed progression

1. Demonstrate the full local library workflow with PNG and JPEG. Include recovery after an interrupted import, malformed images, a locked library, low storage, and keyboard-only navigation.
2. Add smart collections, advanced filters, comparison and duplicate review after individual scope approval. [Smart folders](https://en.eagle.cool/support/article/smart-folders)
3. Expand media coverage by measuring storage, metadata, thumbnail, preview, playback and export for each format and platform. [Supported formats](https://en.eagle.cool/support/article/what-file-formats-does-eagle-support)
4. Evaluate capture, migration and automation. Start migration research with documented export workflows and a separate destination library. D04b defers Eagle migration, browser capture, cloud, AI and additional formats. Plugins and broad media playback are candidates for later stages. [Eaglepack](https://en.eagle.cool/support/article/import-exporting-eaglepacks) · [Public API](https://developer.eagle.cool/web-api)
