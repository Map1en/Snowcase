use super::*;

#[test]
fn item_ids_come_from_booth_links_or_plain_numbers() {
    for (text, id) in [
        ("https://booth.pm/ja/items/5058077", Some(5058077)),
        ("https://jingo1016.booth.pm/items/5058077?utm=x", Some(5058077)),
        ("booth.pm/items/123", Some(123)),
        (" 42 ", Some(42)),
        ("https://example.com/items/5058077", None),
        ("https://booth.pm.example.com/items/1", None),
        ("https://booth.pm/ja/browse/3D", None),
        ("", None),
    ] {
        assert_eq!(item_id(text), id, "{text}");
    }
}

#[test]
fn a_page_gives_its_name_shop_link_and_first_picture() {
    let json = r#"{
        "id": 7, "name": "Sample Outfit", "url": "https://shop.booth.pm/items/7",
        "description": "ignored", "price": "¥ 1,000",
        "shop": {"name": "Sample Shop", "subdomain": "shop"},
        "images": [{"original": "https://booth.pximg.net/a/1.jpg", "resized": "x"}, {"original": "https://booth.pximg.net/a/2.jpg"}]
    }"#;

    assert_eq!(
        parse_item(json).unwrap(),
        BoothItem {
            id: 7,
            name: "Sample Outfit".into(),
            shop: "Sample Shop".into(),
            url: "https://shop.booth.pm/items/7".into(),
            image: Some("https://booth.pximg.net/a/1.jpg".into()),
        }
    );
    assert!(parse_item("<html>").is_err());
}
