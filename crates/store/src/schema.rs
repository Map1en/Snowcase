// @generated automatically by Diesel CLI.

diesel::table! {
    blob (id) {
        id -> BigInt,
        hash -> Nullable<Text>,
        size -> BigInt,
        fingerprint -> Text,
        kind -> Text,
    }
}

diesel::table! {
    blob_location (id) {
        id -> BigInt,
        blob_id -> BigInt,
        path -> Text,
        state -> Text,
        last_seen -> BigInt,
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
        category -> Text,
        status -> Text,
        note -> Text,
        cover_media -> Nullable<Text>,
        created_at -> BigInt,
        updated_at -> BigInt,
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
    tag (id) {
        id -> BigInt,
        name -> Text,
    }
}

diesel::table! {
    version (id) {
        id -> BigInt,
        item_id -> BigInt,
        label -> Nullable<Text>,
        released_at -> Nullable<BigInt>,
        added_at -> BigInt,
    }
}

diesel::table! {
    version_file (version_id, blob_id) {
        version_id -> BigInt,
        blob_id -> BigInt,
        file_name -> Text,
        part -> Nullable<Text>,
        variant -> Nullable<Text>,
        role -> Nullable<Text>,
    }
}

diesel::joinable!(blob_location -> blob (blob_id));
diesel::joinable!(item -> media (cover_media));
diesel::joinable!(item_folder -> folder (folder_id));
diesel::joinable!(item_folder -> item (item_id));
diesel::joinable!(item_image -> item (item_id));
diesel::joinable!(item_image -> media (media_hash));
diesel::joinable!(item_source -> item (item_id));
diesel::joinable!(item_tag -> item (item_id));
diesel::joinable!(item_tag -> tag (tag_id));
diesel::joinable!(version -> item (item_id));
diesel::joinable!(version_file -> blob (blob_id));
diesel::joinable!(version_file -> version (version_id));

diesel::allow_tables_to_appear_in_same_query!(
    blob,
    blob_location,
    folder,
    item,
    item_folder,
    item_image,
    item_source,
    item_tag,
    media,
    tag,
    version,
    version_file,
);
