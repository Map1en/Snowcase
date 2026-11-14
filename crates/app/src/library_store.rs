use gpui_kit::*;
use snowcase_core::StoreError;
use snowcase_core::library::Library;

pub struct LibraryStore {
    library: Option<Library>,
    error: Option<StoreError>,
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
                error: Some(error),
            },
        }
    }

    pub fn library(&self) -> Option<&Library> {
        self.library.as_ref()
    }

    pub fn error(&self) -> Option<&StoreError> {
        self.error.as_ref()
    }

    pub fn change<T>(
        &mut self,
        cx: &mut Context<Self>,
        change: impl FnOnce(&mut Library) -> Result<T, StoreError>,
    ) -> Option<T> {
        let library = self.library.as_mut()?;
        self.error = None;
        let value = change(library).map_err(|error| self.error = Some(error)).ok();
        cx.notify();
        value
    }
}
