# Patched iced_tiny_skia

This crate is vendored from `pop-os/libcosmic` commit
`caec74c2559924443f12fc6faf97a5bcefe6271d`.

The local compositor patch treats a transient softbuffer/viewport size mismatch
as a lost surface. Iced then recreates the surface at the current size instead
of panicking while constructing a TinySkia pixel map.
