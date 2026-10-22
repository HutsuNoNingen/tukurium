# Avatar assets

Copied from `tonarium-prototype/client/public/oekaki/` on 2026-10-22.
Source checkout HEAD: `a62b482234d9c202044141df357793f6e7223b8f`.

The source attribution reads:

> avatar.glb（元: anim@RESO_Pera_idle.glb）・sample.png・template.png は
> https://github.com/anthrotech-dev/oekaki-avatar から作者の許可を得て使用しています。

`sample.png` is unchanged. `avatar.glb` has its Draco mesh streams decoded by
`scripts/convert-avatar.cjs` because the Godot importer does not support them.
Its 14 meshes, node hierarchy, UVs and 7 animation clips are retained.
The converter accepts the source GLB, destination GLB, and a `draco3d` module path.
This prototype uses a new Godot renderer and does not reuse
Tonarium's networking, accounts, or application message formats.
