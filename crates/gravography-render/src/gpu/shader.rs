//! WGSL module assembly (`CLAUDE.md` §6): shaders are plain `.wgsl` files in `src/gpu/shaders/`, combined by
//! ordered concatenation. Each module may use items from the modules before it. WGSL has no imports, so the order
//! is the dependency order.

/// One WGSL source module.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ShaderModule {
    /// File name (for headers and error messages).
    pub name: &'static str,
    /// WGSL source.
    pub source: &'static str,
}

macro_rules! wgsl_module {
    ($name:literal) => {
        ShaderModule {
            name: $name,
            source: include_str!(concat!("shaders/", $name)),
        }
    };
}

/// Shared constants and helpers (status codes, finiteness test).
pub const COMMON: ShaderModule = wgsl_module!("common.wgsl");
/// Kerr–Schild metric fields and derivatives (port of `kerr_schild.rs`).
pub const KERR_SCHILD: ShaderModule = wgsl_module!("kerr_schild.wgsl");
/// Hamiltonian right-hand side (port of `geodesic.rs`).
pub const GEODESIC: ShaderModule = wgsl_module!("geodesic.wgsl");
/// RK4 step (port of `ode.rs::rk4_step`).
pub const RK4: ShaderModule = wgsl_module!("rk4.wgsl");
/// DOPRI5 step, error norm and step controller (port of `ode.rs`).
pub const DOPRI5: ShaderModule = wgsl_module!("dopri5.wgsl");
/// Single-ray tracer with step caps and termination (port of `ray.rs`, DOPRI5 and RK4).
pub const RAY: ShaderModule = wgsl_module!("ray.wgsl");
/// Camera rays: projection, tetrad, initial momentum (ports of `camera.rs`, `Tetrad`, `camera_ray_momentum`).
pub const CAMERA: ShaderModule = wgsl_module!("camera.wgsl");
/// Per-pixel trace pass writing the G-buffer.
pub const TRACE_PIXELS: ShaderModule = wgsl_module!("trace_pixels.wgsl");
/// Entry points for the parity harness.
pub const TEST_RAYS: ShaderModule = wgsl_module!("test_rays.wgsl");

/// Concatenates `modules` in order, each preceded by a `// ---- module: <name> ----` header line.
pub fn assemble(modules: &[ShaderModule]) -> String {
    let mut out = String::new();
    for m in modules {
        out.push_str("// ---- module: ");
        out.push_str(m.name);
        out.push_str(" ----\n");
        out.push_str(m.source);
        if !m.source.ends_with('\n') {
            out.push('\n');
        }
    }
    out
}

/// Modules of the test-ray shader, in dependency order.
pub const TEST_RAY_MODULES: [ShaderModule; 7] = [COMMON, KERR_SCHILD, GEODESIC, RK4, DOPRI5, RAY, TEST_RAYS];

/// Modules of the per-pixel trace shader, in dependency order.
pub const TRACE_PIXEL_MODULES: [ShaderModule; 8] = [
    COMMON,
    KERR_SCHILD,
    GEODESIC,
    RK4,
    DOPRI5,
    RAY,
    CAMERA,
    TRACE_PIXELS,
];

/// Every assembled shader (name, modules), for validation tests.
pub const ALL_SHADERS: [(&str, &[ShaderModule]); 2] = [
    ("test_rays", &TEST_RAY_MODULES),
    ("trace_pixels", &TRACE_PIXEL_MODULES),
];

/// Source of the test-ray shader (entry points `trace_rk4_main`, `trace_dopri5_main`, `eval_rhs_main`).
pub fn test_ray_shader() -> String {
    assemble(&TEST_RAY_MODULES)
}

/// Source of the per-pixel trace shader (entry point `trace_pixels_main`).
pub fn trace_pixel_shader() -> String {
    assemble(&TRACE_PIXEL_MODULES)
}

#[cfg(test)]
mod tests {
    use super::*;
    use wgpu::naga;

    /// Parses and validates WGSL with naga (no GPU needed), returning the module or a readable error.
    fn validate(src: &str) -> Result<naga::Module, String> {
        let module = naga::front::wgsl::parse_str(src).map_err(|e| e.emit_to_string(src))?;
        naga::valid::Validator::new(
            naga::valid::ValidationFlags::all(),
            naga::valid::Capabilities::empty(),
        )
        .validate(&module)
        .map_err(|e| format!("{e:?}"))?;
        Ok(module)
    }

    #[test]
    fn assembly_preserves_order_and_headers() {
        let a = ShaderModule {
            name: "a.wgsl",
            source: "const A: f32 = 1.0;",
        };
        let b = ShaderModule {
            name: "b.wgsl",
            source: "const B: f32 = A;\n",
        };
        let s = assemble(&[a, b]);
        assert_eq!(
            s,
            "// ---- module: a.wgsl ----\nconst A: f32 = 1.0;\n// ---- module: b.wgsl ----\nconst B: f32 = A;\n"
        );
        assert!(validate(&s).is_ok());
    }

    #[test]
    fn shaders_are_valid_wgsl() {
        let entries = |src: &str| -> Vec<String> {
            let module = validate(src).unwrap_or_else(|e| panic!("{e}"));
            module.entry_points.iter().map(|e| e.name.clone()).collect()
        };
        let t = entries(&test_ray_shader());
        for e in ["trace_rk4_main", "trace_dopri5_main", "eval_rhs_main"] {
            assert!(t.iter().any(|n| n == e), "missing entry point {e}");
        }
        assert_eq!(
            entries(&trace_pixel_shader()),
            vec!["trace_pixels_main".to_string()]
        );
    }

    #[test]
    fn every_module_is_assembled() {
        let mut all: Vec<&str> = Vec::new();
        for (name, modules) in ALL_SHADERS {
            let names: Vec<&str> = modules.iter().map(|m| m.name).collect();
            let mut sorted = names.clone();
            sorted.sort_unstable();
            sorted.dedup();
            assert_eq!(sorted.len(), names.len(), "{name} lists a module twice");
            all.extend(names);
        }
        // Every .wgsl file in the shader directory is part of an assembly (no stale modules).
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src/gpu/shaders");
        for entry in std::fs::read_dir(dir).unwrap() {
            let file = entry.unwrap().file_name().into_string().unwrap();
            assert!(
                all.contains(&file.as_str()),
                "{file} is not assembled into any shader"
            );
        }
    }
}
