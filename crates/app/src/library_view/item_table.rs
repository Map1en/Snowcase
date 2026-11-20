use gpui_kit::component::table::{Column, TableDelegate, TableState};
use gpui_kit::*;
use snowcase_core::library::Snapshot;
use snowcase_core::ItemId;

use super::format_size;
use crate::i18n::tr;

pub struct ItemRow {
    pub id: ItemId,
    cells: [SharedString; 4],
}

impl ItemRow {
    pub fn new(library: &Snapshot, id: ItemId) -> Option<Self> {
        let item = library.item(id)?;
        let facts = library.facts(id).cloned().unwrap_or_default();
        let tags = library
            .tags_of(id)
            .iter()
            .map(|tag| tag.name.as_str())
            .collect::<Vec<_>>()
            .join(", ");
        Some(Self {
            id,
            cells: [
                item.title.clone().into(),
                tags.into(),
                facts.file_count.to_string().into(),
                format_size(facts.size).into(),
            ],
        })
    }
}

#[derive(Default)]
pub struct ItemTable {
    pub rows: Vec<ItemRow>,
}

impl TableDelegate for ItemTable {
    fn columns_count(&self, _: &App) -> usize {
        4
    }

    fn rows_count(&self, _: &App) -> usize {
        self.rows.len()
    }

    fn column(&self, ix: usize, cx: &App) -> Column {
        match ix {
            0 => Column::new("name", tr!(cx, "item.name")).width(px(280.)),
            1 => Column::new("tags", tr!(cx, "item.tags")).width(px(180.)),
            2 => Column::new("files", tr!(cx, "item.files")).width(px(60.)).text_right(),
            _ => Column::new("size", tr!(cx, "item.size")).width(px(90.)).text_right(),
        }
    }

    fn render_td(
        &mut self,
        row: usize,
        col: usize,
        _: &mut Window,
        _: &mut Context<TableState<Self>>,
    ) -> impl IntoElement {
        let text = self
            .rows
            .get(row)
            .and_then(|row| row.cells.get(col))
            .cloned()
            .unwrap_or_default();
        div().truncate().child(text)
    }
}
