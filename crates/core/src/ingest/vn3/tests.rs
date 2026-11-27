use super::*;

const GENERATOR_FOOTER: &str = "この利用規約は、VN3ライセンスVer.1.10に準拠したVN3ライセンス利用規約ジェネレータVer.1.11を利用して、サンプルによって作成されました。";
const TEMPLATE_FOOTER: &str =
    "この利用規約は、VN3ライセンスVer.1.10のテンプレートを利用して、サンプルによって作成されました。";

const HEADINGS: [(usize, &str); 8] = [
    (0, "（1）利用主体"),
    (2, "（2）オンラインサービスへのアップロード"),
    (5, "（3）センシティブな表現"),
    (8, "（4）加工"),
    (12, "（5）再配布・配布"),
    (14, "（6）メディア・プロダクトへの利用"),
    (18, "（7）二次創作"),
    (21, "（8）その他"),
];

const TYPICAL: [&str; 23] = [
    "営利・非営利の目的問わず利用を許可します",
    "権利者に個別に問い合わせて下さい",
    "許可します",
    "許可します",
    "許可しません",
    "許可します（ただし棲み分けはおこなうこと）",
    "許可します（ただし棲み分けはおこなうこと）",
    "許可しません",
    "許可します",
    "許可します",
    "許可します",
    "ユーザー間で行うことを許可します",
    "許可しません",
    "許可しません",
    "許可します",
    "許可します",
    "権利者に個別に問い合わせて下さい",
    "権利者に個別に問い合わせて下さい",
    "営利・非営利の目的問わず配布等（頒布、送信を含む）を許可します",
    "営利・非営利の目的問わず配布等（頒布、送信を含む）を許可します",
    "営利・非営利の目的問わず配布等（頒布、送信を含む）を許可します",
    "不要ですがあると嬉しいです",
    "許可しません",
];

struct Sample {
    answers: [String; 23],
    letters: bool,
    skip: Option<Vn3Item>,
    notes: String,
    footer: &'static str,
}

impl Sample {
    fn typical() -> Self {
        Self {
            answers: TYPICAL.map(String::from),
            letters: true,
            skip: None,
            notes: "なし".into(),
            footer: GENERATOR_FOOTER,
        }
    }

    fn answer(mut self, item: Vn3Item, answer: &str) -> Self {
        self.answers[item as usize] = answer.into();
        self
    }

    fn render(&self) -> String {
        let mut out = String::from(
            "（参考資料）簡易一覧\n 1. 利用主体   A 個人利用   B 法人利用\n 許可しません 許可しません\n\n\
             個別条件\n 1. 許諾対象データ\n 2. 利用条件\n 3. 特記事項\n 4. 権利者および権利者への問い合わせ先\n\n\
             基本条項\n 第１条（語の定義）\n\n個別条件\n1. 許諾対象データ\nサンプル衣装\n2. 利用条件\n",
        );
        for item in Vn3Item::ALL {
            let index = item as usize;
            if let Some((_, heading)) = HEADINGS.iter().find(|(at, _)| *at == index) {
                out.push_str(heading);
                out.push('\n');
            }
            if self.skip == Some(item) {
                continue;
            }
            let letter = if self.letters { format!("{}. ", item.letter()) } else { String::new() };
            out.push_str(&format!(
                "                   {letter}〔説明〕{}\n                           {}\n",
                item.anchor(),
                self.answers[index]
            ));
            if item == Vn3Item::CorporateUse {
                out.push_str("上記の利用の許可には、自ら利用することへの許可が含まれます。\n");
            }
        }
        let lead = if self.letters { "X. " } else { "" };
        out.push_str(&format!(
            "3. 特記事項\n {lead}特記事項\n {}\n4. 権利者および権利者への問い合わせ先\n権利者：サンプル\n\
             8. 利用規約のバージョン\n1.00\n{}\n",
            self.notes, self.footer
        ));
        out
    }
}

fn parse(sample: &Sample) -> Vn3License {
    parse_vn3(&sample.render()).expect("recognized as VN3")
}

fn answer(license: &Vn3License, item: Vn3Item) -> Vn3Answer {
    term(license, item).answer
}

fn term(license: &Vn3License, item: Vn3Item) -> &Vn3Term {
    license.terms.iter().find(|t| t.item == item).unwrap()
}

#[test]
fn generator_license_reads_versions_and_every_item() {
    let license = parse(&Sample::typical());

    assert_eq!(license.license_version, "1.10");
    assert_eq!(license.origin, Vn3Origin::Generator { version: "1.11".into() });
    assert_eq!(license.notes, None);
    let answers: Vec<_> = license.terms.iter().map(|t| t.answer).collect();
    use Vn3Answer::*;
    assert_eq!(
        answers,
        [
            AnyPurpose, AskOwner, Allow, Allow, Deny, AllowSegregated, AllowSegregated, Deny,
            Allow, Allow, Allow, BetweenUsers, Deny, Deny, Allow, Allow, AskOwner, AskOwner,
            DistributeAnyPurpose, DistributeAnyPurpose, DistributeAnyPurpose, CreditAppreciated,
            Deny,
        ]
    );
    assert_eq!(term(&license, Vn3Item::Credit).raw, "不要ですがあると嬉しいです");
}

#[test]
fn template_copy_with_zero_width_spaces_and_no_letters_is_read() {
    let mut sample = Sample::typical();
    sample.letters = false;
    sample.footer = TEMPLATE_FOOTER;
    let text = sample.render().replace("許可", "\u{200b}許\u{200b}可").replace("\n", "\u{200b}\n");

    let license = parse_vn3(&text).expect("recognized as VN3");

    assert_eq!(license.origin, Vn3Origin::Template);
    assert_eq!(answer(&license, Vn3Item::PersonalUse), Vn3Answer::AnyPurpose);
    assert_eq!(answer(&license, Vn3Item::Outsourcing), Vn3Answer::BetweenUsers);
    assert!(license.terms.iter().all(|t| t.answer != Vn3Answer::Unknown));
}

#[test]
fn the_longest_official_option_wins_across_line_breaks_and_bracket_widths() {
    let sample = Sample::typical()
        .answer(
            Vn3Item::ThirdPartyUpload,
            "対象を限定しての公開を許可します(例えばVRChatにてPrivateでの公開の許可を含\n      みます)",
        )
        .answer(Vn3Item::SexualExpression, "許可します")
        .answer(
            Vn3Item::PoliticalOrReligiousUse,
            "許可しません（ただし私的使用（プライベートな範囲での利用）については禁止しません）",
        )
        .answer(Vn3Item::Redistribution, "無償に限りユーザー間で行うことを許可します")
        .answer(Vn3Item::ModifiedDistribution, "無償に限り本利用規約に従わせることを条件に許可します")
        .answer(Vn3Item::MeshReuse, "該当するデータではありません");

    let license = parse(&sample);

    assert_eq!(answer(&license, Vn3Item::ThirdPartyUpload), Vn3Answer::LimitedAudience);
    assert_eq!(answer(&license, Vn3Item::SexualExpression), Vn3Answer::Allow);
    assert_eq!(answer(&license, Vn3Item::ViolentExpression), Vn3Answer::AllowSegregated);
    assert_eq!(answer(&license, Vn3Item::PoliticalOrReligiousUse), Vn3Answer::DenyExceptPrivate);
    assert_eq!(answer(&license, Vn3Item::Redistribution), Vn3Answer::FreeOnlyBetweenUsers);
    assert_eq!(answer(&license, Vn3Item::ModifiedDistribution), Vn3Answer::FreeOnlyBoundByTerms);
    assert_eq!(answer(&license, Vn3Item::MeshReuse), Vn3Answer::NotApplicable);
}

#[test]
fn distinguishable_media_answer_accepts_any_official_reason() {
    let sample = Sample::typical()
        .answer(
            Vn3Item::VideoAndStreaming,
            "オリジナルと異なることが分かる程度に改変した場合は許可します（公式配信と誤解されないため）",
        )
        .answer(
            Vn3Item::Publications,
            "オリジナルと異なることが分かる程度に改変した場合は許可します(公式配信と誤解されないため)",
        )
        .answer(
            Vn3Item::Merchandise,
            "オリジナルと異なることが分かる程度に改変した場合は許可します（公式グッズと誤解されないため）",
        );

    let license = parse(&sample);

    for item in [Vn3Item::VideoAndStreaming, Vn3Item::Publications, Vn3Item::Merchandise] {
        assert_eq!(answer(&license, item), Vn3Answer::AllowIfDistinguishable, "{item:?}");
    }
    assert_eq!(answer(&license, Vn3Item::SoftwareEmbedding), Vn3Answer::AskOwner);
}

#[test]
fn answers_outside_the_items_options_are_unknown_and_keep_their_text() {
    let sample = Sample::typical()
        .answer(Vn3Item::SocialPlatformUpload, "許可します（ただしVRChatに限ります）")
        .answer(Vn3Item::Adjustment, "ユーザー間で行うことを許可します")
        .answer(Vn3Item::TransferOfRights, "許可しますが事前に連絡してください");

    let license = parse(&sample);

    let social = term(&license, Vn3Item::SocialPlatformUpload);
    assert_eq!(social.answer, Vn3Answer::Unknown);
    assert_eq!(social.raw, "許可します（ただしVRChatに限ります）");
    assert_eq!(answer(&license, Vn3Item::GamePlatformUpload), Vn3Answer::Allow);
    let adjustment = term(&license, Vn3Item::Adjustment);
    assert_eq!(adjustment.answer, Vn3Answer::Unknown);
    assert_eq!(adjustment.raw, "ユーザー間で行うことを許可します");
    assert_eq!(answer(&license, Vn3Item::TransferOfRights), Vn3Answer::Allow);
}

#[test]
fn a_section_heading_pushed_before_the_answer_by_columns_is_skipped() {
    let text = Sample::typical()
        .render()
        .replace("（1）利用主体\n", "（1）\n")
        .replace("個人による利用\n", "個人による利用\n利用主体 ");

    let license = parse_vn3(&text).expect("recognized as VN3");

    let personal = term(&license, Vn3Item::PersonalUse);
    assert_eq!(personal.answer, Vn3Answer::AnyPurpose);
    assert_eq!(personal.raw, "営利・非営利の目的問わず利用を許可します");
}

#[test]
fn a_missing_item_is_unknown_and_the_rest_still_parse() {
    let mut sample = Sample::typical();
    sample.skip = Some(Vn3Item::UseForOtherModels);

    let license = parse(&sample);

    let missing = term(&license, Vn3Item::UseForOtherModels);
    assert_eq!(missing.answer, Vn3Answer::Unknown);
    assert_eq!(missing.raw, "");
    assert_eq!(answer(&license, Vn3Item::Outsourcing), Vn3Answer::BetweenUsers);
    assert_eq!(answer(&license, Vn3Item::TransferOfRights), Vn3Answer::Deny);
}

#[test]
fn special_notes_keep_their_lines_and_empty_notes_are_none() {
    let mut sample = Sample::typical();
    sample.notes = "①NFTでの利用は禁止します。\n ②商用利用時は   クレジットを記載してください。".into();
    assert_eq!(
        parse(&sample).notes.as_deref(),
        Some("①NFTでの利用は禁止します。\n②商用利用時は クレジットを記載してください。")
    );

    sample.notes = "特になし".into();
    assert_eq!(parse(&sample).notes, None);
}

#[test]
fn texts_without_the_japanese_vn3_footer_are_not_vn3() {
    assert_eq!(parse_vn3("Copyright (c) 2018\nPermission is hereby granted, free of charge"), None);
    let english = Sample::typical()
        .render()
        .replace(GENERATOR_FOOTER, "This license was created using VN3 License Generator Ver.1.11.");
    assert_eq!(parse_vn3(&english), None);
}
