use super::*;

#[test]
fn parent_node_climbs_folders_then_containers_then_the_file_and_the_item() {
    let item = ItemId(1);
    let at = |path: &str| FacetNode {
        item_id: item,
        blob_id: Some(snowcase_store::BlobId(2)),
        inner_path: Some(path.to_owned()),
    };
    let mut node = at("Manuka/Outfit.unitypackage!/Assets/Shop/Black.prefab");
    let mut chain = Vec::new();
    while let Some(parent) = parent_node(&node) {
        chain.push(parent.inner_path.clone().unwrap_or_else(|| {
            if parent.blob_id.is_some() { "<file>" } else { "<item>" }.to_owned()
        }));
        node = parent;
    }

    assert_eq!(
        chain,
        [
            "Manuka/Outfit.unitypackage!/Assets/Shop",
            "Manuka/Outfit.unitypackage!/Assets",
            "Manuka/Outfit.unitypackage",
            "Manuka",
            "<file>",
            "<item>",
        ]
    );
}
