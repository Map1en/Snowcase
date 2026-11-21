use super::*;

fn of(title: &str, files: &[&str]) -> (Option<&'static str>, Vec<&'static str>) {
    let found = recognize(title, files);
    (found.category, found.targets)
}

#[test]
fn outfits_name_the_avatars_they_fit() {
    assert_eq!(
        of("NekoRibbonPoncho", &["Mafuyu.Cian_NekoRibbonPoncho_v1.0.0.unitypackage"]),
        (Some("outfit"), vec!["Cian", "Mafuyu"])
    );
    assert_eq!(of("Plumery_Bikini_Mafuyu", &["Plumery_Bikini_Mafuyu.zip"]), (Some("outfit"), vec!["Mafuyu"]));
    assert_eq!(of("mafuyu的afternoon裙子", &[]).1, vec!["Mafuyu"]);
}

#[test]
fn only_the_official_romanized_name_counts_not_nicknames() {
    assert_eq!(of("和桜セーラー", &["和桜セーラー_真冬.zip"]).1, Vec::<&str>::new());
    assert_eq!(of("shoolelegan_mahuyu", &[]).1, Vec::<&str>::new());
}

#[test]
fn a_short_avatar_name_only_counts_as_a_whole_word() {
    assert_eq!(of("PLUMARIUM", &["PLUMARIUM.unitypackage"]).1, Vec::<&str>::new());
    assert_eq!(of("Ribbon_Plum", &["Ribbon_Plum.unitypackage"]), (Some("accessory"), vec!["Plum"]));
}

#[test]
fn an_avatar_name_in_a_picture_does_not_mean_it_fits() {
    assert_eq!(of("Choker", &["Choker.unitypackage", "Lime.png"]), (Some("accessory"), vec![]));
}

#[test]
fn an_avatar_itself_is_a_body_and_fits_nobody() {
    assert_eq!(of("Mafuyu", &["Mafuyu_v1.0.7.unitypackage", "Mafuyu_VRM.vrm"]), (Some(AVATAR), vec![]));
    assert_eq!(of("Kipfel", &["Kipfel_1.1.0.zip"]), (Some(AVATAR), vec![]));
}

#[test]
fn kinds_come_from_plain_words_in_priority_order() {
    assert_eq!(of("Mafuyu_Hairstyle", &[]).0, Some("hair"));
    assert_eq!(of("poi_Toon", &["poi_Toon_7.3.50_UpTo_9.0.60.zip"]).0, Some("shader"));
    assert_eq!(of("RealKissSystem", &[]).0, Some("gimmick"));
    assert_eq!(of("Oyasumisan_glasses", &[]).0, Some("accessory"));
    assert_eq!(of("hottaglasses_fullset", &[]).0, Some("accessory"));
    assert_eq!(of("VRC-Gesture-Manager-master", &[]).0, Some("tool"));
    assert_eq!(of("Hair_Ribbon", &[]).0, Some("accessory"));
    assert_eq!(of("NekoRibbonPoncho", &[]).0, Some("outfit"));
}

#[test]
fn nothing_is_guessed_without_a_known_word() {
    assert_eq!(of("stripe", &["stripe.zip"]), (None, vec![]));
    assert_eq!(of("白猫通りのナギ亭", &[]), (None, vec![]));
}

#[test]
fn the_title_decides_the_kind_before_the_files_do() {
    assert_eq!(of("Dark_Bloom_Dress", &["Hair_Ribbon.unitypackage"]).0, Some("outfit"));
    assert_eq!(of("Dark_Bloom", &["Dark_Bloom_Dress.unitypackage"]).0, Some("outfit"));
}
