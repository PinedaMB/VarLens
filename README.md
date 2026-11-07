# VarLens

VarLens es una aplicación de escritorio nativa para Linux desarrollada con **Rust**, **GTK4** y **Libadwaita**, orientada a desarrolladores que requieren:
1. **Formateo y validación de JSON:** Modos Prettify, Minify y validación de sintaxis con preservación del orden de claves (`serde_json`).
2. **Conversor de `var_dump()` de PHP a JSON:** Parser formal basado en gramática **Pest** capaz de transformar estructuras recursivas y anidadas (arrays secuenciales/asociativos, objetos con modificadores de visibilidad `private`/`protected`, tipos primitivos y referencias).
3. **Comparador visual de Diff (Side-by-Side y Unificado):** Motor de cálculo con el crate `similar` (algoritmos Myers y Patience), resaltando visualmente adiciones en verde (`.success`) y supresiones en rojo (`.error`) con desplazamiento sincronizado entre paneles.
4. **Concurrencia no bloqueante:** Todo cálculo intensivo (>1000 líneas) se procesa en hilos secundarios (`std::thread`) y se comunica de vuelta al hilo principal de la UI usando el canal de despacho de `glib::MainContext`.

---

## 🛠️ Stack Tecnológico
- **Lenguaje:** Rust 2021
- **UI:** GTK4 (v4.12+) + Libadwaita (v1.5+)
- **Editor:** GtkSourceView 5 (`sourceview5`) con numeración de líneas, gutters y soporte para temas claro/oscuro.
- **Diff:** Crate `similar` (Myers & Patience)
- **Parser:** `pest` & `pest_derive` (con gramática PEG recursiva)
- **JSON:** `serde_json` con feature `preserve_order`

---

## 🚀 Compilación y Ejecución

```bash
cargo run
```

Para ejecutar los tests unitarios:

```bash
cargo test
```
