use std::collections::hash_map::HashMap;
use std::path::{Component, Path, PathBuf};

use boa_engine::gc::GcRefCell;
use boa_engine::module::{ModuleLoader, Referrer};
use boa_engine::prelude::*;
use boa_engine::{JsResult, js_string};

pub struct ModLoader {
    root: PathBuf,
    module_map: GcRefCell<HashMap<PathBuf, Module>>,
}

impl ModLoader {
    pub fn new(root: PathBuf) -> Self {
        assert!(root.is_absolute(), "relative path is not allowed!");

        Self {
            root,
            module_map: GcRefCell::default(),
        }
    }

    #[inline]
    pub fn insert(&self, path: PathBuf, module: Module) {
        self.module_map.borrow_mut().insert(path, module);
    }

    #[inline]
    pub fn get(&self, path: &Path) -> Option<Module> {
        self.module_map.borrow().get(path).cloned()
    }
}

impl ModuleLoader for ModLoader {
    fn load_imported_module(
        &self,
        referrer: Referrer,
        specifier: JsString,
        finish_load: Box<dyn FnOnce(JsResult<Module>, &mut Context)>,
        ctx: &mut Context,
    ) {
        fn path_outside_root() -> JsError {
            JsError::from_opaque(js_string!("path is outside the module root").into())
        }

        fn f(
            this: &ModLoader,
            referrer: Referrer,
            specifier: JsString,
            ctx: &mut Context,
        ) -> JsResult<Module> {
            let specifier = specifier.to_std_string_escaped();
            let spec = PathBuf::from(&specifier);
            let mut path;
            if spec.is_absolute() {
                path = spec;
                if !path.starts_with(&this.root) {
                    return Err(path_outside_root());
                }
            } else {
                path = referrer.path().map_or(PathBuf::new(), Path::to_owned);
                for c in spec.components() {
                    if c != Component::ParentDir {
                        path.push(c);
                    } else if path.as_os_str().is_empty() {
                        return Err(path_outside_root());
                    } else {
                        path.pop();
                    }
                }
                drop(spec);
                path = this.root.join(path);
            }

            if let Some(m) = this.get(&path) {
                return Ok(m);
            }

            let source = Source::from_filepath(&path).map_err(|e| {
                JsNativeError::typ()
                    .with_message(format!("could not open file `{specifier}`"))
                    .with_cause(JsError::from_rust(e))
            })?;
            let module = Module::parse(source, None, ctx).map_err(|e| {
                JsNativeError::typ()
                    .with_message(format!("could not parse module `{specifier}`"))
                    .with_cause(e)
            })?;
            this.insert(path, module.clone());
            Ok(module)
        }

        finish_load(f(self, referrer, specifier, ctx), ctx);
    }

    fn register_module(&self, specifier: JsString, module: Module) {
        self.insert(PathBuf::from(specifier.to_std_string_escaped()), module);
    }

    fn get_module(&self, specifier: JsString) -> Option<Module> {
        self.get(specifier.to_std_string_escaped().as_ref())
    }
}
