# Third-party material

Data and other material bundled in this repository that is not original to Gravography, with source and licence
(`docs/PRD.md` PL-6). Rust crate dependencies are not listed here; see `Cargo.lock`.

| Item | Path | Source | Licence |
|---|---|---|---|
| CIE 1931 2° standard observer colour-matching functions (360–830 nm, 1 nm), unmodified, with the CIE metadata file | `assets/cie/CIE_xyz_1931_2deg.csv`, `assets/cie/CIE_xyz_1931_2deg.csv_metadata.json` | CIE 2019, *Colour-matching functions of CIE 1931 standard colorimetric observer*, International Commission on Illumination (CIE), Vienna, AT, DOI [10.25039/CIE.DS.xvudnb9b](https://doi.org/10.25039/CIE.DS.xvudnb9b); downloaded 2026-10-30 from `https://files.cie.co.at/Publications-datasets/CIE_xyz_1931_2deg.csv` (MD5 `17cca777db64b17170f06f67ce9d3ab7`, matching the metadata) | [CC BY-SA 4.0](https://creativecommons.org/licenses/by-sa/4.0/) (per the metadata `rightsList`) |

The CIE table is embedded into the `gravography-sky` library at build time (`include_str!`). It is redistributed
unmodified, with attribution; the CC BY-SA 4.0 terms apply to the data file itself, not to the surrounding code.

## Fonts embedded in the binary

The viewer's user interface (egui) embeds fonts from the `epaint_default_fonts` crate (version 0.37.0, enabled
through egui's `default_fonts` feature since WP-11). They are compiled into the `gravography` binary but not
stored in this repository. Crate licence: `(MIT OR Apache-2.0) AND OFL-1.1 AND Ubuntu-font-1.0`. The licence
texts ship with the crate (`fonts/*.txt` in the crate source).

| Font | Use | Licence |
|---|---|---|
| Ubuntu Light (Canonical / Dalton Maag) | proportional UI text | Ubuntu Font Licence 1.0 |
| Hack Regular (Source Foundry, from DejaVu / Bitstream Vera) | monospace text (HUD) | MIT; Bitstream Vera Licence; DejaVu changes public domain |
| Noto Emoji Regular (Google) | emoji fallback | SIL Open Font Licence 1.1 |
| emoji-icon-font and its subset egui-icons (John Slegers) | UI icons | MIT |

## Rust dependencies

The crates in `Cargo.lock` are used under their own licences, all permissive (MIT, Apache-2.0, BSD, Zlib, Unlicense,
Unicode-3.0 and similar, plus the font licences above). Run `cargo tree -e normal --prefix none --format '{p} {l}' | sort -u` for the list.
