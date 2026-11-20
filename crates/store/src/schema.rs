// @generated automatically by Diesel CLI.

diesel::table! {
    blob (id) {
        id -> BigInt,
        hash -> Nullable<Text>,
        size -> BigInt,
        fingerprint -> Text,
        crc32 -> Nullable<BigInt>,
        kind -> Text,
    }
}

diesel::table! {
    blob_location (id) {
        id -> BigInt,
        blob_id -> BigInt,
        root_id -> Nullable<BigInt>,
        rel_path -> Text,
        state -> Text,
        last_seen -> BigInt,
    }
}

diesel::table! {
    facet (id) {
        id -> BigInt,
        item_id -> BigInt,
        blob_id -> Nullable<BigInt>,
        inner_path -> Nullable<Text>,
        key -> Text,
        value -> Text,
        target_item_id -> Nullable<BigInt>,
        origin -> Text,
        removed -> Bool,
    }
}

diesel::table! {
    folder (id) {
        id -> BigInt,
        parent_id -> Nullable<BigInt>,
        name -> Text,
        position -> Integer,
    }
}

diesel::table! {
    item (id) {
        id -> BigInt,
        title -> Text,
        note -> Text,
        cover_media -> Nullable<Text>,
        created_at -> BigInt,
        updated_at -> BigInt,
    }
}

diesel::table! {
    item_file (item_id, blob_id) {
        item_id -> BigInt,
        blob_id -> BigInt,
        file_name -> Text,
        display_name -> Nullable<Text>,
        added_at -> BigInt,
        extracted_from_blob_id -> Nullable<BigInt>,
        extracted_from_path -> Nullable<Text>,
    }
}

diesel::table! {
    item_folder (item_id, folder_id) {
        item_id -> BigInt,
        folder_id -> BigInt,
    }
}

diesel::table! {
    item_image (item_id, media_hash) {
        item_id -> BigInt,
        media_hash -> Text,
        position -> Integer,
        origin -> Text,
    }
}

diesel::table! {
    item_source (id) {
        id -> BigInt,
        item_id -> BigInt,
        store -> Text,
        url -> Nullable<Text>,
        external_id -> Nullable<Text>,
        author -> Nullable<Text>,
        account -> Nullable<Text>,
    }
}

diesel::table! {
    item_tag (item_id, tag_id) {
        item_id -> BigInt,
        tag_id -> BigInt,
    }
}

diesel::table! {
    media (hash) {
        hash -> Text,
        mime -> Text,
        width -> Nullable<Integer>,
        height -> Nullable<Integer>,
        bytes -> BigInt,
    }
}

diesel::table! {
    root (id) {
        id -> BigInt,
        path -> Text,
    }
}

diesel::table! {
    tag (id) {
        id -> BigInt,
        name -> Text,
    }
}

diesel::joinable!(blob_location -> blob (blob_id));
diesel::joinable!(blob_location -> root (root_id));
diesel::joinable!(item -> media (cover_media));
diesel::joinable!(item_file -> item (item_id));
diesel::joinable!(item_folder -> folder (folder_id));
diesel::joinable!(item_folder -> item (item_id));
diesel::joinable!(item_image -> item (item_id));
diesel::joinable!(item_image -> media (media_hash));
diesel::joinable!(item_source -> item (item_id));
diesel::joinable!(item_tag -> item (item_id));
diesel::joinable!(item_tag -> tag (tag_id));

diesel::allow_tables_to_appear_in_same_query!(
    blob,
    blob_location,
    facet,
    folder,
    item,
    item_file,
    item_folder,
    item_image,
    item_source,
    item_tag,
    media,
    root,
    tag,
);
