use super::*;

const CHINESE: CodePage = CodePage::SimplifiedChinese;

#[test]
fn shift_jis_names_misread_as_gbk_are_repaired() {
    let cases = [
        ("僄僋僗僥乕儖両.unitypackage", "エクステール！.unitypackage"),
        ("僼傽乕僺傾僗_ver1.00", "ファーピアス_ver1.00"),
        ("偒偣偐偊曽朄.txt", "きせかえ方法.txt"),
        ("怓堘偄.png", "色違い.png"),
        ("ReadMe_僉僾僼僃儖愢柧彂(jp).url", "ReadMe_キプフェル説明書(jp).url"),
    ];
    for (broken, fixed) in cases {
        assert_eq!(repair_mojibake(broken, CHINESE).as_deref(), Some(fixed), "{broken}");
    }
}

#[test]
fn correct_japanese_chinese_and_ascii_names_are_left_alone() {
    for name in [
        "ゆるさいどぽに.unitypackage",
        "エクリプスヘイロー",
        "おさがりぱじゃま for Chiffon",
        "慈愛の怪盗セット",
        "年末調整2024kan.zip",
        "已适配Demicrown",
        "24美赛经验分享资料",
        "白真冬.zip",
        "mafuyu的afternoon裙子.unitypackage",
        "Outfit_v1.0.zip",
    ] {
        assert_eq!(repair_mojibake(name, CHINESE), None, "{name}");
    }
}

#[test]
fn kanji_only_mojibake_is_missed_on_purpose() {
    assert_eq!(repair_mojibake("婯栺", CHINESE), None);
}

#[test]
fn japanese_and_other_code_pages_never_repair() {
    assert_eq!(repair_mojibake("僄僋僗僥乕儖両", CodePage::Japanese), None);
    assert_eq!(repair_mojibake("僄僋僗僥乕儖両", CodePage::Other), None);
    assert_eq!(CodePage::from_ansi(936), CHINESE);
    assert_eq!(CodePage::from_ansi(932), CodePage::Japanese);
    assert_eq!(CodePage::from_ansi(1252), CodePage::Other);
}

#[test]
fn version_of_reads_the_notations_found_in_real_folders() {
    let cases = [
        ("ARI_M_01_v1.7.3", "1.7.3"),
        ("Chibi_Mafuyu_ver.1.00", "1.00"),
        ("Oyasumisan_glasses_V2.2.zip", "2.2"),
        ("Kipfel_1.1.0.zip", "1.1.0"),
        ("Ribbon1.2.zip", "1.2"),
        ("heyagi08_tokuA_ver1.05_20250131.zip", "1.05"),
        ("Karin_v1.11_20230331", "1.11"),
        ("RalivDynamicPenetrationSystemV1_31", "1.31"),
        ("RealKissSystemV1.2.1", "1.2.1"),
        ("SunaoShader_1_6_2", "1.6.2"),
        ("forKipfel_ACC_NERUZine_prisonya_1_1.zip", "1.1"),
        ("strayKingCat_Mafuyu_cianver1.0.0", "1.0.0"),
        ("rbs_ma-suimin-system-2_v1.1.0-rc.1", "1.1.0-rc.1"),
        ("MD_FANBOX_202503_ver1_0.zip", "1.0"),
        ("UTS2GUI_v.2.0.8_JapaneseEdition", "2.0.8"),
        ("MERODROPS_mafuyu_1.2.unitypackage.zip", "1.2"),
        ("ねずみのくるみ3Dモデル1.01.zip", "1.01"),
        ("Mafuyu.Cian_NekoRibbonPoncho_v1.0.0 (1).zip", "1.0.0"),
    ];
    for (name, version) in cases {
        assert_eq!(version_of(name).as_deref(), Some(version), "{name}");
    }
}

#[test]
fn names_without_a_version_have_none() {
    for name in ["Mafuyu_Hairstyle.zip", "Silver2", "VRCX-0.psd", "3DMGAME", "Shinano", "20250131.zip"] {
        assert_eq!(version_of(name), None, "{name}");
    }
}

fn same_key(a: &str, b: &str) {
    let key = name_key(a);
    assert!(key.is_some(), "{a} has no key");
    assert_eq!(key, name_key(b), "{a} vs {b}");
}

#[test]
fn versions_duplicates_roles_and_avatars_do_not_change_the_key() {
    same_key("Outfit_v1.0.zip", "Outfit_v1.1");
    same_key("Mafuyu.Cian_NekoRibbonPoncho_v1.0.0.unitypackage", "_NekoRibbonPoncho_Materials_v1.0.0.unitypackage");
    same_key("HonmeiKnit_Prefab_Shinano_v1.0", "HonmeiKnit_Material_v1.0.zip");
    same_key("mini_Shirts_Bikini_Karin", "mini_Shirts_Bikini_Shinano.zip");
    same_key("stripe (1).zip", "stripe.zip");
    same_key("rbs_ma-suimin-system-2_v1.0.1.zip", "rbs_ma-suimin-system-2_v1.1.0-rc.1");
    same_key("YumekawaKaren_FullSet_Ver1.3.zip", "YumekawaKaren_Tex.zip");
    same_key("heyagi08_tokuA_ver1.05_20250131", "heyagi08_tokuA_ver1.24_20260914.zip");
    same_key("ねずみのくるみ3Dモデル1.01.zip", "ねずみのくるみ3Dモデル1.10");
    same_key("LopEarMine_PSD.psd", "LopEarMine_Material_v1.1.zip");
    same_key("strayKingCat_Mafuyu_cianver1.0.0", "strayKingCat.zip");
}

#[test]
fn different_products_keep_different_keys() {
    assert_ne!(name_key("AngelicHeart_Fullset_Ver1.0.3.zip"), name_key("SailorLoliRemake_Fullset_Ver2.3.0.zip"));
    assert_ne!(name_key("Ribbon1.2.zip"), name_key("kipfel_ribbon_texture.zip"));
}

#[test]
fn keys_that_are_too_generic_or_only_an_avatar_are_not_offered() {
    for name in [
        "Material.zip",
        "Textures.zip",
        "Shinano",
        "kipfel",
        "Mafuyu_v1.0.6",
        "1.zip",
        "2.zip",
        "kipfel_ribbon_texture.zip",
    ] {
        assert_eq!(name_key(name), None, "{name}");
    }
}

#[test]
fn titles_drop_extensions_versions_and_duplicate_suffixes_but_keep_avatar_names() {
    let cases = [
        ("Mafuyu_v1.0.7.zip", "Mafuyu"),
        ("Mafuyu.Cian_NekoRibbonPoncho_v1.0.0 (1).zip", "Mafuyu.Cian_NekoRibbonPoncho"),
        ("heyagi08_tokuA_ver1.05_20250131.zip", "heyagi08_tokuA"),
        ("ねずみのくるみ3Dモデル1.01.zip", "ねずみのくるみ3Dモデル"),
        ("Kipfel_1.1.0", "Kipfel"),
        ("20250131.zip", "20250131"),
        ("Pitti Muffler", "Pitti Muffler"),
        ("UTS2GUI_v.2.0.8_JapaneseEdition.zip", "UTS2GUI_JapaneseEdition"),
        ("UnlitWF_20230401_01_Core_2019.unitypackage", "UnlitWF_01_Core_2019"),
        ("【AREN】 Kipfel cloth Texture_A", "【AREN】 Kipfel cloth Texture_A"),
        ("シュガーワンピ【フルセット】v2.6.unitypackage", "シュガーワンピ【フルセット】"),
        ("[Ene Collection] Sailor Maiden Mafuyu v1.0.unitypackage", "[Ene Collection] Sailor Maiden Mafuyu"),
    ];
    for (name, title) in cases {
        assert_eq!(title_of(name), title, "{name}");
    }
}

#[test]
fn tag_keys_fold_width_case_and_surrounding_space() {
    assert_eq!(tag_key(" ＢＬＡＣＫ "), "black");
    assert_eq!(tag_key("Black"), tag_key("black"));
    assert_eq!(tag_key("黑丝 "), tag_key("黑丝"));
    assert_ne!(tag_key("黑丝"), tag_key("黑丝袜"));
    assert_eq!(tag_key("ﾊｰﾄ"), tag_key("ハート"));
}

#[test]
fn words_split_camel_case_digits_and_wide_text() {
    assert_eq!(words("NekoRibbonPoncho"), ["Neko", "Ribbon", "Poncho"]);
    assert_eq!(words("forKipfel"), ["for", "Kipfel"]);
    assert_eq!(words("VRCXNoty"), ["VRCX", "Noty"]);
    assert_eq!(words("heyagi08"), ["heyagi", "08"]);
    assert_eq!(words("mafuyu的afternoon"), ["mafuyu", "的", "afternoon"]);
}
