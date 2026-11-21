use super::*;

fn asset(guid: &str, path: &str) -> PackageAsset {
    PackageAsset {
        guid: guid.into(),
        path: path.into(),
        is_file: true,
        has_preview: false,
    }
}

fn key(fingerprint: &str, member: &str) -> PackageKey {
    PackageKey {
        fingerprint: fingerprint.into(),
        member: member.into(),
    }
}

#[test]
fn saved_packages_and_failures_load_back_after_reopening() {
    let dir = tempfile::tempdir().unwrap();
    let scanned = vec![
        ScannedPackage {
            key: key("aa", ""),
            result: Ok(vec![asset("01", "Assets/Shop/Hair/Hair.prefab"), asset("02", "Assets/Shop/Hair/Hair.png")]),
        },
        ScannedPackage {
            key: key("bb", "Outfit/Outfit.unitypackage"),
            result: Ok(vec![asset("03", "Assets/Shop/Outfit/Outfit.prefab")]),
        },
        ScannedPackage {
            key: key("cc", ""),
            result: Err("not a gzip file".into()),
        },
    ];
    Cache::open(dir.path()).unwrap().save_packages(&scanned).unwrap();

    let loaded = Cache::open(dir.path()).unwrap().load_packages().unwrap();

    assert_eq!(loaded, scanned);
}

#[test]
fn saving_a_package_again_replaces_its_assets() {
    let dir = tempfile::tempdir().unwrap();
    let mut cache = Cache::open(dir.path()).unwrap();
    let first = ScannedPackage {
        key: key("aa", ""),
        result: Ok(vec![asset("01", "Assets/Old.prefab")]),
    };
    let second = ScannedPackage {
        key: key("aa", ""),
        result: Ok(vec![asset("02", "Assets/New.prefab")]),
    };

    cache.save_packages(&[first]).unwrap();
    cache.save_packages(std::slice::from_ref(&second)).unwrap();

    assert_eq!(cache.load_packages().unwrap(), [second]);
}

#[test]
fn a_cache_from_another_version_is_thrown_away() {
    let dir = tempfile::tempdir().unwrap();
    {
        let mut cache = Cache::open(dir.path()).unwrap();
        cache
            .save_packages(&[ScannedPackage {
                key: key("aa", ""),
                result: Ok(vec![]),
            }])
            .unwrap();
        cache.conn.batch_execute("PRAGMA user_version = 999;").unwrap();
    }

    let mut cache = Cache::open(dir.path()).unwrap();

    assert!(cache.load_packages().unwrap().is_empty());
}
