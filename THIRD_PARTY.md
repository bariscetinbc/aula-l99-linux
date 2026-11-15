# Credits and distribution scope

This is an unofficial Linux implementation, not an AULA product. AULA names
identify compatible hardware; no vendor endorsement is implied.

Protocol research references:
- https://github.com/gavindi/Aula_L99_Linux
- https://github.com/gavindi/Aula_L99_Linux/blob/main/tools/aula_l99_hacky/re_notes/system_monitor_block.md

The Python transport is an independent implementation informed by those notes.
The GNU GPL version 2 or later license covers this project's source, not vendor firmware, drivers,
trademarks, product photographs, or third-party packages. The source packager
excludes the local trcc package archive, all vendor executables and reference
images. Do not add those to the public repository without distribution rights.

Rust dependencies retain their respective licenses, as declared in their
packages; Cargo.lock records the dependency versions. Python and Pillow are
system dependencies and are not bundled in the source archive. Before shipping
binaries, collect and include applicable dependency license notices.
