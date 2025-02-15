use std::path::{Path, PathBuf};

use crate::build::{CApiConfig, InstallTarget, LibraryTypes};
use crate::install::LibType;
use crate::target::Target;

#[derive(Debug, Default, Clone)]
pub struct ExtraTargets {
    pub include: Vec<(PathBuf, PathBuf)>,
    pub data: Vec<(PathBuf, PathBuf)>,
}

impl ExtraTargets {
    pub fn setup(
        &mut self,
        capi_config: &CApiConfig,
        root_dir: &Path,
        out_dir: Option<&Path>,
    ) -> anyhow::Result<()> {
        self.include = extra_targets(&capi_config.install.include, root_dir, out_dir)?;
        self.data = extra_targets(&capi_config.install.data, root_dir, out_dir)?;

        Ok(())
    }
}

fn extra_targets(
    targets: &[InstallTarget],
    root_path: &Path,
    root_output: Option<&Path>,
) -> anyhow::Result<Vec<(PathBuf, PathBuf)>> {
    use itertools::*;
    targets
        .iter()
        .filter_map(|t| match t {
            InstallTarget::Asset(paths) => Some(paths.install_paths(root_path)),
            InstallTarget::Generated(paths) => {
                root_output.map(|root_output| paths.install_paths(root_output))
            }
        })
        .flatten_ok()
        .collect()
}

#[derive(Debug, Clone)]
pub struct BuildTargets {
    pub name: String,
    pub include: Option<PathBuf>,
    pub static_lib: Option<PathBuf>,
    pub shared_lib: Option<PathBuf>,
    pub impl_lib: Option<PathBuf>,
    pub debug_info: Option<PathBuf>,
    pub def: Option<PathBuf>,
    pub pc: PathBuf,
    pub target: Target,
    pub extra: ExtraTargets,
}

impl BuildTargets {
    pub fn new(
        name: &str,
        target: &Target,
        targetdir: &Path,
        library_types: LibraryTypes,
        capi_config: &CApiConfig,
    ) -> anyhow::Result<BuildTargets> {
        let pc = targetdir.join(format!("{}.pc", &capi_config.pkg_config.filename));
        let include = if capi_config.header.enabled && capi_config.header.generation {
            Some(targetdir.join(&capi_config.header.name).with_extension("h"))
        } else {
            None
        };

        let Some(file_names) = FileNames::from_target(target, name, targetdir) else {
            return Err(anyhow::anyhow!(
                "The target {}-{} is not supported yet",
                target.os,
                target.env
            ));
        };

        Ok(BuildTargets {
            pc,
            include,
            static_lib: library_types.staticlib.then_some(file_names.static_lib),
            shared_lib: library_types.cdylib.then_some(file_names.shared_lib),
            impl_lib: file_names.impl_lib,
            debug_info: file_names.debug_info,
            def: file_names.def,
            name: name.into(),
            target: target.clone(),
            extra: Default::default(),
        })
    }

    fn lib_type(&self) -> LibType {
        LibType::from_build_targets(self)
    }

    pub fn debug_info_file_name(&self, bindir: &Path, libdir: &Path) -> Option<PathBuf> {
        match self.lib_type() {
            // FIXME: Requires setting split-debuginfo to packed and
            // specifying the corresponding file name convention
            // in BuildTargets::new.
            LibType::So | LibType::Dylib => {
                Some(libdir.join(self.debug_info.as_ref()?.file_name()?))
            }
            LibType::Windows => Some(bindir.join(self.debug_info.as_ref()?.file_name()?)),
        }
    }
}

#[derive(Debug, PartialEq, Eq)]
struct FileNames {
    static_lib: PathBuf,
    shared_lib: PathBuf,
    impl_lib: Option<PathBuf>,
    debug_info: Option<PathBuf>,
    def: Option<PathBuf>,
}

impl FileNames {
    fn from_target(target: &Target, lib_name: &str, targetdir: &Path) -> Option<Self> {
        let (shared_lib, static_lib, impl_lib, debug_info, def) = match target.os.as_str() {
            "none" | "linux" | "freebsd" | "dragonfly" | "netbsd" | "android" | "haiku"
            | "illumos" | "openbsd" | "emscripten" | "hurd" => {
                let static_lib = targetdir.join(format!("lib{lib_name}.a"));
                let shared_lib = targetdir.join(format!("lib{lib_name}.so"));
                (shared_lib, static_lib, None, None, None)
            }
            "macos" | "ios" | "tvos" | "visionos" => {
                let static_lib = targetdir.join(format!("lib{lib_name}.a"));
                let shared_lib = targetdir.join(format!("lib{lib_name}.dylib"));
                (shared_lib, static_lib, None, None, None)
            }
            "windows" => {
                if target.env == "msvc" {
                    let shared_lib = targetdir.join(format!("{lib_name}.dll"));
                    let static_lib = targetdir.join(format!("{lib_name}.lib"));
                    let impl_lib = targetdir.join(format!("{lib_name}.dll.lib"));
                    let pdb = targetdir.join(format!("{lib_name}.pdb"));
                    let def = targetdir.join(format!("{lib_name}.def"));

                    (shared_lib, static_lib, Some(impl_lib), Some(pdb), Some(def))
                } else {
                    // FIXME: `dll_prefix` should be `lib` on `*-windows-gnu` targets.
                    // https://github.com/rust-lang/rust/pull/94872#discussion_r825219902
                    let shared_lib = targetdir.join(format!("{lib_name}.dll"));
                    let static_lib = targetdir.join(format!("lib{lib_name}.a"));
                    let impl_lib = targetdir.join(format!("lib{lib_name}.dll.a"));

                    (shared_lib, static_lib, Some(impl_lib), None, None)
                }
            }
            _ => return None,
        };

        Some(Self {
            static_lib,
            shared_lib,
            impl_lib,
            debug_info,
            def,
        })
    }
}

#[cfg(test)]
mod test {
    use std::path::{Path, PathBuf};

    use super::{FileNames, Target};

    #[test]
    fn unix() {
        for os in [
            "none",
            "linux",
            "freebsd",
            "dragonfly",
            "netbsd",
            "android",
            "haiku",
            "illumos",
            "emscripten",
            "hurd",
        ] {
            let target = Target {
                is_target_overridden: false,
                arch: String::from(""),
                os: os.to_string(),
                env: String::from(""),
            };
            let file_names = FileNames::from_target(&target, "ferris", Path::new("/foo/bar"));

            let expected = FileNames {
                static_lib: PathBuf::from("/foo/bar/libferris.a"),
                shared_lib: PathBuf::from("/foo/bar/libferris.so"),
                impl_lib: None,
                debug_info: None,
                def: None,
            };

            assert_eq!(file_names.unwrap(), expected);
        }
    }

    #[test]
    fn apple() {
        for os in ["macos", "ios", "tvos", "visionos"] {
            let target = Target {
                is_target_overridden: false,
                arch: String::from(""),
                os: os.to_string(),
                env: String::from(""),
            };
            let file_names = FileNames::from_target(&target, "ferris", Path::new("/foo/bar"));

            let expected = FileNames {
                static_lib: PathBuf::from("/foo/bar/libferris.a"),
                shared_lib: PathBuf::from("/foo/bar/libferris.dylib"),
                impl_lib: None,
                debug_info: None,
                def: None,
            };

            assert_eq!(file_names.unwrap(), expected);
        }
    }

    #[test]
    fn windows_msvc() {
        let target = Target {
            is_target_overridden: false,
            arch: String::from(""),
            os: String::from("windows"),
            env: String::from("msvc"),
        };
        let file_names = FileNames::from_target(&target, "ferris", Path::new("/foo/bar"));

        let expected = FileNames {
            static_lib: PathBuf::from("/foo/bar/ferris.lib"),
            shared_lib: PathBuf::from("/foo/bar/ferris.dll"),
            impl_lib: Some(PathBuf::from("/foo/bar/ferris.dll.lib")),
            debug_info: Some(PathBuf::from("/foo/bar/ferris.pdb")),
            def: Some(PathBuf::from("/foo/bar/ferris.def")),
        };

        assert_eq!(file_names.unwrap(), expected);
    }

    #[test]
    fn windows_gnu() {
        let target = Target {
            is_target_overridden: false,
            arch: String::from(""),
            os: String::from("windows"),
            env: String::from("gnu"),
        };
        let file_names = FileNames::from_target(&target, "ferris", Path::new("/foo/bar"));

        let expected = FileNames {
            static_lib: PathBuf::from("/foo/bar/libferris.a"),
            shared_lib: PathBuf::from("/foo/bar/ferris.dll"),
            impl_lib: Some(PathBuf::from("/foo/bar/libferris.dll.a")),
            debug_info: None,
            def: None,
        };

        assert_eq!(file_names.unwrap(), expected);
    }
}
