# Third-party material

Data and other material bundled in this repository that is not original to Gravography, with source and licence
(`docs/PRD.md` PL-6). Rust crate dependencies are not listed here; see `Cargo.lock`.

| Item | Path | Source | Licence |
|---|---|---|---|
| CIE 1931 2° standard observer colour-matching functions (360–830 nm, 1 nm), unmodified, with the CIE metadata file | `assets/cie/CIE_xyz_1931_2deg.csv`, `assets/cie/CIE_xyz_1931_2deg.csv_metadata.json` | CIE 2019, *Colour-matching functions of CIE 1931 standard colorimetric observer*, International Commission on Illumination (CIE), Vienna, AT, DOI [10.25039/CIE.DS.xvudnb9b](https://doi.org/10.25039/CIE.DS.xvudnb9b); downloaded 2026-10-30 from `https://files.cie.co.at/Publications-datasets/CIE_xyz_1931_2deg.csv` (MD5 `17cca777db64b17170f06f67ce9d3ab7`, matching the metadata) | [CC BY-SA 4.0](https://creativecommons.org/licenses/by-sa/4.0/) (per the metadata `rightsList`) |

The CIE table is embedded into the `gravography-sky` library at build time (`include_str!`). It is redistributed
unmodified, with attribution; the CC BY-SA 4.0 terms apply to the data file itself, not to the surrounding code.
