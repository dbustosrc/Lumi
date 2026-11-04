# Builds de Lumi

- Generar los artefactos de la aplicación exclusivamente mediante `pwsh -NoProfile -File ./build.ps1` desde la raíz del repositorio Lumi.
- No invocar `cargo build` directamente ni sustituir el script por comandos independientes de parada, publicación o limpieza. El script mantiene el orden y la conservación de la última build válida.
- `cargo test`, `cargo check`, `cargo clippy` y las demás comprobaciones siguen siendo herramientas de validación; se aplica la delegación de pruebas indicada en el AGENTS.md raíz.
- Las pruebas del script deben usar copias en directorios desechables y Cargo simulado. No usar pruebas de fallo o limpieza contra builds ni datos del usuario.
