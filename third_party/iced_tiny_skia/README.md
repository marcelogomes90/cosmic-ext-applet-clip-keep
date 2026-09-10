# Patched iced_tiny_skia

This crate is vendored from `pop-os/libcosmic` commit
`a401af8b80f66dcf086c81d0fb116e4d5ca76bc4`.

The local compositor patch treats a transient softbuffer/viewport size mismatch
as a lost surface. Iced then recreates the surface at the current size instead
of panicking while constructing a TinySkia pixel map.
