use super::*;

fn paths(list: &[&str]) -> Vec<String> {
    list.iter().map(|path| (*path).to_owned()).collect()
}

#[test]
fn single_child_folder_chains_collapse_into_one_entry() {
    let paths = paths(&[
        "Assets/PLUSONE/Mafuyu/Mafuyu.prefab",
        "Assets/PLUSONE/Mafuyu/Textures/Body.png",
        "Assets/PLUSONE/Mafuyu/Textures/Hair.png",
        "Assets/lilToon/Shader/lts.shader",
    ]);

    assert_eq!(top(&paths), "Assets");
    let assets = entries(&paths, "Assets");
    assert_eq!(assets.folders, ["Assets/PLUSONE/Mafuyu", "Assets/lilToon/Shader"]);
    let mafuyu = entries(&paths, "Assets/PLUSONE/Mafuyu");
    assert_eq!(mafuyu, Entries {
        folders: vec!["Assets/PLUSONE/Mafuyu/Textures".into()],
        files: vec!["Assets/PLUSONE/Mafuyu/Mafuyu.prefab"],
    });
    assert_eq!(shown("Assets/PLUSONE/Mafuyu", "Assets"), "PLUSONE/Mafuyu");
}

#[test]
fn a_folder_only_holds_paths_below_it_not_siblings_with_the_same_prefix() {
    assert!(holds("Assets/Shop/Hair", "Assets/Shop/Hair/Hair.prefab"));
    assert!(!holds("Assets/Shop/Hair", "Assets/Shop/HairPin/Pin.prefab"));
    let paths = paths(&["Assets/Shop/Hair/Hair.prefab", "Assets/Shop/HairPin/Pin.prefab"]);
    assert_eq!(entries(&paths, "Assets/Shop/Hair").files, ["Assets/Shop/Hair/Hair.prefab"]);
}
