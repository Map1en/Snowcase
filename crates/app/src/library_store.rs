use gpui_kit::*;
use snowcase_core::StoreError;
use snowcase_core::library::Library;

pub struct LibraryStore {
    library: Option<Library>,
    error: Option<String>,
}

impl LibraryStore {
    pub fn new(library: Result<Library, StoreError>) -> Self {
        match library {
            Ok(library) => Self {
                library: Some(library),
                error: None,
            },
            Err(error) => Self {
                library: None,
                error: Some(error.to_string()),
            },
        }
    }

    pub fn library(&self) -> Option<&Library> {
        self.library.as_ref()
    }

    pub fn error(&self) -> Option<&str> {
        self.error.as_deref()
    }

    pub fn change<T>(
        &mut self,
        cx: &mut Context<Self>,
        change: impl FnOnce(&mut Library) -> Result<T, StoreError>,
    ) -> Option<T> {
        let library = self.library.as_mut()?;
        let result = change(library);
        self.error = result.as_ref().err().map(ToString::to_string);
        cx.notify();
        result.ok()
    }
}
