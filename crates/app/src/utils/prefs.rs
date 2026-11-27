use std::fs;
use std::path::PathBuf;

use gpui_kit::{App, Global};
use serde::Serialize;
use serde::de::DeserializeOwned;
use serde_json::{Map, Value};

#[derive(Default)]
struct Prefs {
    path: Option<PathBuf>,
    values: Map<String, Value>,
}

impl Global for Prefs {}

pub fn init(path: Option<PathBuf>, cx: &mut App) {
    let values = path
        .as_ref()
        .and_then(|path| fs::read(path).ok())
        .and_then(|bytes| serde_json::from_slice(&bytes).ok())
        .unwrap_or_default();
    cx.set_global(Prefs { path, values });
}

pub fn get<T: DeserializeOwned>(key: &str, cx: &App) -> Option<T> {
    let value = cx.try_global::<Prefs>()?.values.get(key)?;
    serde_json::from_value(value.clone()).ok()
}

pub fn set<T: Serialize>(key: &str, value: &T, cx: &mut App) {
    let Ok(value) = serde_json::to_value(value) else {
        return;
    };
    let prefs = cx.default_global::<Prefs>();
    if prefs.values.get(key) == Some(&value) {
        return;
    }
    prefs.values.insert(key.to_owned(), value);
    let Some(path) = &prefs.path else {
        return;
    };
    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir).ok();
    }
    if let Ok(bytes) = serde_json::to_vec_pretty(&prefs.values) {
        fs::write(path, bytes).ok();
    }
}

#[cfg(test)]
mod tests;
