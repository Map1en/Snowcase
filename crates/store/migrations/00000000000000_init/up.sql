CREATE TABLE media (
    hash TEXT PRIMARY KEY NOT NULL,
    mime TEXT NOT NULL,
    width INTEGER,
    height INTEGER,
    bytes BIGINT NOT NULL
);

CREATE TABLE item (
    id INTEGER PRIMARY KEY NOT NULL,
    title TEXT NOT NULL,
    category TEXT NOT NULL,
    status TEXT NOT NULL,
    note TEXT NOT NULL DEFAULT '',
    cover_media TEXT REFERENCES media(hash),
    created_at BIGINT NOT NULL,
    updated_at BIGINT NOT NULL
);

CREATE TABLE item_source (
    id INTEGER PRIMARY KEY NOT NULL,
    item_id BIGINT NOT NULL REFERENCES item(id) ON DELETE CASCADE,
    store TEXT NOT NULL,
    url TEXT,
    external_id TEXT,
    author TEXT,
    account TEXT
);
CREATE INDEX item_source_item ON item_source(item_id);
CREATE INDEX item_source_external ON item_source(store, external_id);

CREATE TABLE version (
    id INTEGER PRIMARY KEY NOT NULL,
    item_id BIGINT NOT NULL REFERENCES item(id) ON DELETE CASCADE,
    label TEXT,
    released_at BIGINT,
    added_at BIGINT NOT NULL
);
CREATE INDEX version_item ON version(item_id);

CREATE TABLE blob (
    id INTEGER PRIMARY KEY NOT NULL,
    hash TEXT UNIQUE,
    size BIGINT NOT NULL,
    fingerprint TEXT NOT NULL,
    kind TEXT NOT NULL
);

CREATE TABLE blob_location (
    id INTEGER PRIMARY KEY NOT NULL,
    blob_id BIGINT NOT NULL REFERENCES blob(id) ON DELETE CASCADE,
    path TEXT NOT NULL UNIQUE,
    state TEXT NOT NULL,
    last_seen BIGINT NOT NULL
);
CREATE INDEX blob_location_blob ON blob_location(blob_id);

CREATE TABLE version_file (
    version_id BIGINT NOT NULL REFERENCES version(id) ON DELETE CASCADE,
    blob_id BIGINT NOT NULL REFERENCES blob(id),
    file_name TEXT NOT NULL,
    part TEXT,
    variant TEXT,
    role TEXT,
    PRIMARY KEY (version_id, blob_id)
);
CREATE INDEX version_file_blob ON version_file(blob_id);

CREATE TABLE item_image (
    item_id BIGINT NOT NULL REFERENCES item(id) ON DELETE CASCADE,
    media_hash TEXT NOT NULL REFERENCES media(hash),
    position INTEGER NOT NULL,
    origin TEXT NOT NULL,
    PRIMARY KEY (item_id, media_hash)
);
CREATE INDEX item_image_media ON item_image(media_hash);

CREATE TABLE tag (
    id INTEGER PRIMARY KEY NOT NULL,
    name TEXT NOT NULL UNIQUE COLLATE NOCASE
);

CREATE TABLE item_tag (
    item_id BIGINT NOT NULL REFERENCES item(id) ON DELETE CASCADE,
    tag_id BIGINT NOT NULL REFERENCES tag(id) ON DELETE CASCADE,
    PRIMARY KEY (item_id, tag_id)
);
CREATE INDEX item_tag_tag ON item_tag(tag_id);

CREATE TABLE folder (
    id INTEGER PRIMARY KEY NOT NULL,
    parent_id BIGINT REFERENCES folder(id) ON DELETE CASCADE,
    name TEXT NOT NULL,
    position INTEGER NOT NULL
);
CREATE INDEX folder_parent ON folder(parent_id);

CREATE TABLE item_folder (
    item_id BIGINT NOT NULL REFERENCES item(id) ON DELETE CASCADE,
    folder_id BIGINT NOT NULL REFERENCES folder(id) ON DELETE CASCADE,
    PRIMARY KEY (item_id, folder_id)
);
CREATE INDEX item_folder_folder ON item_folder(folder_id);
