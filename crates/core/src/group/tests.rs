use snowcase_formats::guess_kind;

use super::*;

struct Library {
    files: Vec<FoundFile>,
    listings: HashMap<String, Vec<ArchiveEntry>>,
}

impl Library {
    fn new() -> Self {
        Self {
            files: Vec::new(),
            listings: HashMap::new(),
        }
    }

    fn file(mut self, rel_path: &str, size: u64) -> Self {
        self.files.push(FoundFile {
            rel_path: rel_path.into(),
            size,
            modified_ms: 1,
            kind: guess_kind(rel_path.rsplit_once('.').map(|(_, ext)| ext)),
        });
        self
    }

    fn archive(self, rel_path: &str, members: &[(&str, u64)]) -> Self {
        let size = members.iter().map(|(_, size)| size).sum::<u64>() / 2 + 1;
        let mut this = self.file(rel_path, size);
        this.listings.insert(
            rel_path.into(),
            members
                .iter()
                .map(|(path, size)| ArchiveEntry {
                    path: (*path).into(),
                    size: *size,
                    crc32: 0,
                })
                .collect(),
        );
        this
    }

    fn plan(&self) -> Plan {
        plan(&self.files, &self.listings, CodePage::SimplifiedChinese)
    }
}

fn shape(plan: &Plan) -> Vec<(String, Vec<String>)> {
    plan.groups
        .iter()
        .map(|group| {
            (
                group.title.clone(),
                group.files.iter().map(|file| file.rel_path.clone()).collect(),
            )
        })
        .collect()
}

fn titles(plan: &Plan) -> Vec<&str> {
    plan.groups.iter().map(|group| group.title.as_str()).collect()
}

#[test]
fn a_zip_and_its_extracted_folder_become_one_item_titled_after_the_zip() {
    let library = Library::new()
        .archive(
            "Dark_Bloom_Dress.zip",
            &[("Dark_Bloom_Dress/Dark_Bloom_Dress.unitypackage", 900), ("Dark_Bloom_Dress/body.png", 100)],
        )
        .file("Dark_Bloom_Dress/Dark_Bloom_Dress/Dark_Bloom_Dress.unitypackage", 900)
        .file("Dark_Bloom_Dress/Dark_Bloom_Dress/body.png", 100);

    let plan = library.plan();

    assert_eq!(titles(&plan), ["Dark_Bloom_Dress"]);
    let extracted: Vec<_> = plan.groups[0]
        .files
        .iter()
        .map(|file| file.extracted_from.as_ref().map(|(archive, member)| (archive.as_str(), member.as_str())))
        .collect();
    assert_eq!(
        extracted,
        [
            None,
            Some(("Dark_Bloom_Dress.zip", "Dark_Bloom_Dress/Dark_Bloom_Dress.unitypackage")),
            Some(("Dark_Bloom_Dress.zip", "Dark_Bloom_Dress/body.png")),
        ]
    );
}

#[test]
fn folders_named_after_file_types_rejoin_their_zip_even_without_a_signal_of_their_own() {
    let library = Library::new()
        .archive(
            "Ribbon1.2.zip",
            &[("Ribbon.unitypackage", 500), ("PSD/Ribbon_tartan.psd", 3000), ("FBX/ribbon.fbx", 50), ("texture/ribbon.png", 80)],
        )
        .file("PSD/Ribbon_tartan.psd", 3000)
        .file("FBX/ribbon.fbx", 50)
        .file("texture/ribbon.png", 80);

    let plan = library.plan();

    assert_eq!(plan.groups.len(), 1);
    assert_eq!(plan.groups[0].title, "Ribbon");
    assert_eq!(plan.groups[0].files.len(), 4);
}

#[test]
fn different_versions_stay_apart_and_carry_their_version() {
    let library = Library::new()
        .file("Outfit_v1.0.unitypackage", 10)
        .file("Outfit_v1.1.unitypackage", 11)
        .archive("stripe.zip", &[("stripe.unitypackage", 5)])
        .archive("stripe (1).zip", &[("stripe.unitypackage", 5)]);

    let plan = library.plan();

    let mut found: Vec<_> = plan
        .groups
        .iter()
        .map(|group| (group.title.clone(), group.version.clone(), group.files.len()))
        .collect();
    found.sort();
    assert_eq!(
        found,
        [
            ("Outfit".into(), Some("1.0".into()), 1),
            ("Outfit".into(), Some("1.1".into()), 1),
            ("stripe".into(), None, 2),
        ]
    );
}

fn grouped(plan: &Plan) -> Vec<Vec<String>> {
    let mut groups: Vec<Vec<String>> = plan
        .groups
        .iter()
        .map(|group| group.files.iter().map(|file| file.rel_path.clone()).collect())
        .collect();
    groups.sort();
    groups
}

#[test]
fn parts_of_one_version_and_unversioned_parts_still_join() {
    let library = Library::new()
        .file("HonmeiKnit_Material_v1.0.unitypackage", 10)
        .file("HonmeiKnit_Prefab_Shinano_v1.0.unitypackage", 11)
        .file("YumekawaKaren_FullSet_Ver1.3.unitypackage", 12)
        .file("YumekawaKaren_Tex.unitypackage", 13);

    let plan = library.plan();

    assert_eq!(
        grouped(&plan),
        [
            vec!["HonmeiKnit_Material_v1.0.unitypackage".to_owned(), "HonmeiKnit_Prefab_Shinano_v1.0.unitypackage".to_owned()],
            vec!["YumekawaKaren_FullSet_Ver1.3.unitypackage".to_owned(), "YumekawaKaren_Tex.unitypackage".to_owned()],
        ]
    );
}

#[test]
fn an_unversioned_part_does_not_chain_two_versions_together() {
    let library = Library::new()
        .file("RibbonKit_v1.0.unitypackage", 10)
        .file("RibbonKit_Tex.unitypackage", 11)
        .file("RibbonKit_v2.0.unitypackage", 12);

    let plan = library.plan();

    assert_eq!(
        grouped(&plan),
        [
            vec!["RibbonKit_Tex.unitypackage".to_owned(), "RibbonKit_v1.0.unitypackage".to_owned()],
            vec!["RibbonKit_v2.0.unitypackage".to_owned()],
        ]
    );
}

#[test]
fn a_folder_holding_several_archives_is_split_into_its_children() {
    let library = Library::new()
        .archive("mafuyu-meeting/MafuyuMini_ver1.012.zip", &[("MafuyuMini_ver1.012.unitypackage", 400)])
        .archive("mafuyu-meeting/santa_boushi_AB_ver1.00.zip", &[("santa_boushi_AB_ver1.00.unitypackage", 300)])
        .file("mafuyu-meeting/MafuyuMini_ver1.012/MafuyuMini_ver1.012.unitypackage", 400)
        .file("mafuyu-meeting/MafuyuMini_ver1.012/readme.pdf", 10)
        .file("mafuyu-meeting/santa_boushi_AB_ver1.00/santa_boushi_AB_ver1.00.unitypackage", 300);

    let plan = library.plan();

    assert_eq!(
        shape(&plan),
        [
            (
                "MafuyuMini".into(),
                vec![
                    "mafuyu-meeting/MafuyuMini_ver1.012.zip".into(),
                    "mafuyu-meeting/MafuyuMini_ver1.012/MafuyuMini_ver1.012.unitypackage".into(),
                    "mafuyu-meeting/MafuyuMini_ver1.012/readme.pdf".into(),
                ]
            ),
            (
                "santa_boushi_AB".into(),
                vec![
                    "mafuyu-meeting/santa_boushi_AB_ver1.00.zip".into(),
                    "mafuyu-meeting/santa_boushi_AB_ver1.00/santa_boushi_AB_ver1.00.unitypackage".into(),
                ]
            ),
        ]
    );
}

#[test]
fn a_folder_with_several_packages_but_at_most_one_archive_stays_whole() {
    let library = Library::new()
        .file("mochi_bob1.4/mochi_bob.unitypackage", 10)
        .file("mochi_bob1.4/material.unitypackage", 10)
        .file("mochi_bob1.4/clip,psd/mochi_bob.psd", 10);

    let plan = library.plan();

    assert_eq!(titles(&plan), ["mochi_bob"]);
    assert_eq!(plan.groups[0].files.len(), 3);
}

#[test]
fn an_avatar_folder_with_packages_in_subfolders_stays_whole() {
    let library = Library::new()
        .file("Mafuyu_v1.0.7/Unitypackage/Mafuyu_v1.0.7.unitypackage", 900)
        .file("Mafuyu_v1.0.7/Unitypackage/Mafuyu_Another_v1.0.7.unitypackage", 900)
        .file("Mafuyu_v1.0.7/Textures/Body.psd", 300)
        .file("Mafuyu_v1.0.7/VRM/Mafuyu_VRM.vrm", 600);

    let plan = library.plan();

    assert_eq!(titles(&plan), ["Mafuyu"]);
    assert_eq!(plan.groups[0].files.len(), 4);
}

#[test]
fn an_archive_without_a_signal_does_not_swallow_the_products_it_bundles() {
    let library = Library::new()
        .archive("bundle/mafuyu-meeting.7z", &[("MafuyuMini.zip", 400), ("apron.zip", 300)])
        .archive("bundle/MafuyuMini.zip", &[("MafuyuMini.unitypackage", 390)])
        .archive("bundle/apron.zip", &[("apron_accessories.unitypackage", 290)]);

    let plan = library.plan();

    assert_eq!(titles(&plan), ["MafuyuMini", "apron"]);
}

#[test]
fn a_name_that_is_only_an_avatar_does_not_merge_with_the_avatar_itself() {
    let library = Library::new()
        .file("Shinano/Ohanyo_Lingerie_Shinano_v1.0.unitypackage", 50)
        .archive("Shinano_ver1.01.2.zip", &[("Shinano_ver1.01.2/Shinano.unitypackage", 800)]);

    let plan = library.plan();

    assert_eq!(titles(&plan), ["Shinano", "Shinano"]);
}

#[test]
fn a_generic_zip_joins_its_extracted_package_but_nothing_else() {
    let library = Library::new()
        .archive("Material.zip", &[("Material.unitypackage", 404)])
        .file("Material.unitypackage", 404)
        .archive("Textures.zip", &[("Textures/body.png", 900), ("Textures/body.psd", 900)]);

    let plan = library.plan();

    assert_eq!(
        shape(&plan),
        [
            ("Material".into(), vec!["Material.unitypackage".into(), "Material.zip".into()]),
            ("Textures".into(), vec!["Textures.zip".into()]),
        ]
    );
}

#[test]
fn obvious_non_assets_are_left_out_and_rar_is_reported() {
    let library = Library::new()
        .file("setup.exe", 10)
        .archive("Inter-4.1.zip", &[("Inter.ttf", 30)])
        .archive("emotions.zip", &[("happy.png", 30)])
        .file("photo.png", 10)
        .file("TENSHI_CHOKER.rar", 10)
        .file("VRCX_noty.psd", 10);

    let plan = library.plan();

    assert_eq!(titles(&plan), ["VRCX_noty"]);
    assert_eq!(plan.unsupported, ["TENSHI_CHOKER.rar"]);
}

#[test]
fn mojibake_extracted_names_still_match_their_archive_and_get_a_display_name() {
    let library = Library::new()
        .archive("エクステール_.zip", &[("エクステール！.unitypackage", 77)])
        .file("僄僋僗僥乕儖両.unitypackage", 77);

    let plan = library.plan();

    assert_eq!(plan.groups.len(), 1);
    let extracted = plan.groups[0].files.iter().find(|file| file.extracted_from.is_some()).unwrap();
    assert_eq!(extracted.display_name.as_deref(), Some("エクステール！.unitypackage"));
}
