# Icon-Only Plugin Example

This package has no executable. Once installed, its transparent PNGs are available in the library's example icon-pack folder. Uninstalling removes the mount but retains key references; reinstalling restores resolution.

Package it from the repository root:

```powershell
pnpm plugin:pack -- examples/icon-pack .local/example-icon-pack.m18plugin
```

Glyphs are Apache-2.0 licensed, resized, and recolored as light transparent PNGs. The repository's canonical license text is `licenses/Apache-2.0.txt`; the generated package includes a root `LICENSE-APACHE-2.0.txt` copy. Use `pnpm plugin:pack:icon-example` to build that package.
