use std::sync::LazyLock;

use regex::Regex;
use unicode_normalization::UnicodeNormalization;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Vn3License {
    pub license_version: String,
    pub origin: Vn3Origin,
    pub terms: Vec<Vn3Term>,
    pub notes: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Vn3Origin {
    Generator { version: String },
    Template,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Vn3Term {
    pub item: Vn3Item,
    pub answer: Vn3Answer,
    pub raw: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Vn3Item {
    PersonalUse,
    CorporateUse,
    SocialPlatformUpload,
    GamePlatformUpload,
    ThirdPartyUpload,
    SexualExpression,
    ViolentExpression,
    PoliticalOrReligiousUse,
    Adjustment,
    Modification,
    UseForOtherModels,
    Outsourcing,
    Redistribution,
    ModifiedDistribution,
    VideoAndStreaming,
    Publications,
    Merchandise,
    SoftwareEmbedding,
    MeshReuse,
    CompatibleCreation,
    DerivativeWork,
    Credit,
    TransferOfRights,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Vn3Answer {
    Allow,
    Deny,
    AskOwner,
    NonCommercialOnly,
    NonCommercialOrCostRecovery,
    AnyPurpose,
    PublicAllowed,
    LimitedAudience,
    AllowSegregated,
    DenyExceptPrivate,
    BetweenUsers,
    FreeOnly,
    BoundByTerms,
    FreeOnlyBoundByTerms,
    FreeOnlyBetweenUsers,
    AllowIfDistinguishable,
    PrivateSelfOnly,
    DistributeNonCommercial,
    DistributeNonCommercialOrCostRecovery,
    DistributeAnyPurpose,
    CreationDenied,
    DistributionDenied,
    NotApplicable,
    CreditRequired,
    CreditAppreciated,
    CreditNotRequired,
    Unknown,
}

type Options = &'static [(&'static str, Vn3Answer)];

const ASK: (&str, Vn3Answer) = ("権利者に個別に問い合わせて下さい", Vn3Answer::AskOwner);
const ALLOW: (&str, Vn3Answer) = ("許可します", Vn3Answer::Allow);
const DENY: (&str, Vn3Answer) = ("許可しません", Vn3Answer::Deny);
const DENY_EXCEPT_PRIVATE: (&str, Vn3Answer) = (
    "許可しません（ただし私的使用（プライベートな範囲での利用）については禁止しません）",
    Vn3Answer::DenyExceptPrivate,
);

const BASIC: Options = &[ALLOW, DENY, ASK];
const SUBJECT: Options = &[
    DENY,
    ("非営利目的に限り許可します", Vn3Answer::NonCommercialOnly),
    ("非営利および非営利有償目的での利用を許可します", Vn3Answer::NonCommercialOrCostRecovery),
    ("営利・非営利の目的問わず利用を許可します", Vn3Answer::AnyPurpose),
    ASK,
];
const THIRD_PARTY: Options = &[
    ("許可します（例えばVRChatにてPublicでの公開の許可を含みます）", Vn3Answer::PublicAllowed),
    ("対象を限定しての公開を許可します（例えばVRChatにてPrivateでの公開の許可を含みます）", Vn3Answer::LimitedAudience),
    DENY,
    ASK,
];
const SENSITIVE: Options = &[
    ALLOW,
    ("許可します（ただし棲み分けはおこなうこと）", Vn3Answer::AllowSegregated),
    DENY_EXCEPT_PRIVATE,
    DENY,
    ASK,
];
const POLITICAL: Options = &[ALLOW, DENY, DENY_EXCEPT_PRIVATE, ASK];
const OUTSOURCING: Options = &[ALLOW, ("ユーザー間で行うことを許可します", Vn3Answer::BetweenUsers), DENY, ASK];
const DISTRIBUTION: Options = &[
    ALLOW,
    ("無償に限り許可します", Vn3Answer::FreeOnly),
    ("本利用規約に従わせることを条件に許可します", Vn3Answer::BoundByTerms),
    ("無償に限り本利用規約に従わせることを条件に許可します", Vn3Answer::FreeOnlyBoundByTerms),
    ("ユーザー間で行うことを許可します", Vn3Answer::BetweenUsers),
    ("無償に限りユーザー間で行うことを許可します", Vn3Answer::FreeOnlyBetweenUsers),
    DENY,
    ASK,
];
const DISTINGUISHABLE: &str = "オリジナルと異なることが分かる程度に改変した場合は許可します";
const MEDIA: Options = &[ALLOW, (DISTINGUISHABLE, Vn3Answer::AllowIfDistinguishable), DENY, ASK];
const DISTRIBUTE_NON_COMMERCIAL: (&str, Vn3Answer) =
    ("非営利目的での配布等（頒布、送信を含む）を許可します", Vn3Answer::DistributeNonCommercial);
const DISTRIBUTE_COST_RECOVERY: (&str, Vn3Answer) = (
    "非営利および非営利有償目的での配布等（頒布、送信を含む）を許可します",
    Vn3Answer::DistributeNonCommercialOrCostRecovery,
);
const DISTRIBUTE_ANY: (&str, Vn3Answer) =
    ("営利・非営利の目的問わず配布等（頒布、送信を含む）を許可します", Vn3Answer::DistributeAnyPurpose);
const REUSE: Options = &[
    ("私的かつ本人のみによる利用に限り許可します", Vn3Answer::PrivateSelfOnly),
    DISTRIBUTE_NON_COMMERCIAL,
    DISTRIBUTE_COST_RECOVERY,
    DISTRIBUTE_ANY,
    ("作成を許可しません", Vn3Answer::CreationDenied),
    ("該当するデータではありません", Vn3Answer::NotApplicable),
    ASK,
];
const DERIVATIVE: Options = &[
    DISTRIBUTE_NON_COMMERCIAL,
    DISTRIBUTE_COST_RECOVERY,
    DISTRIBUTE_ANY,
    ("配布等（頒布、送信を含む）を許可しません", Vn3Answer::DistributionDenied),
    ASK,
];
const CREDIT: Options = &[
    ("必要です", Vn3Answer::CreditRequired),
    ("不要ですがあると嬉しいです", Vn3Answer::CreditAppreciated),
    ("不要です", Vn3Answer::CreditNotRequired),
    ASK,
];

impl Vn3Item {
    pub const ALL: [Self; 23] = [
        Self::PersonalUse,
        Self::CorporateUse,
        Self::SocialPlatformUpload,
        Self::GamePlatformUpload,
        Self::ThirdPartyUpload,
        Self::SexualExpression,
        Self::ViolentExpression,
        Self::PoliticalOrReligiousUse,
        Self::Adjustment,
        Self::Modification,
        Self::UseForOtherModels,
        Self::Outsourcing,
        Self::Redistribution,
        Self::ModifiedDistribution,
        Self::VideoAndStreaming,
        Self::Publications,
        Self::Merchandise,
        Self::SoftwareEmbedding,
        Self::MeshReuse,
        Self::CompatibleCreation,
        Self::DerivativeWork,
        Self::Credit,
        Self::TransferOfRights,
    ];

    pub fn letter(self) -> char {
        char::from(b'A' + self as u8)
    }

    fn anchor(self) -> &'static str {
        match self {
            Self::PersonalUse => "個人による利用",
            Self::CorporateUse => "法人による利用",
            Self::SocialPlatformUpload => "cluster等を含みます）へアップロードすること",
            Self::GamePlatformUpload => "オンラインゲームプラットフォームへアップロードすること",
            Self::ThirdPartyUpload => "やオンラインゲームプラットフォームへアップロードすること",
            Self::SexualExpression => "性的表現への利用",
            Self::ViolentExpression => "暴力的表現への利用",
            Self::PoliticalOrReligiousUse => "宗教活動への利用",
            Self::Adjustment => "ファイル形式を変換すること",
            Self::Modification => "主たる素体として、改変を行う場合を含みます）",
            Self::UseForOtherModels => "他のモデルを主たる素体として改変を行う場合を含みます）",
            Self::Outsourcing => "委託のために本データを貸与すること",
            Self::Redistribution => "未改変状態での再配布",
            Self::ModifiedDistribution => "改変した本データを配布すること",
            Self::VideoAndStreaming => "放送への利用",
            Self::Publications => "電子出版物への利用",
            Self::Merchandise => "（グッズ）への利用",
            Self::SoftwareEmbedding => "容易に取り出せない状態にして配布すること",
            Self::MeshReuse => "改変が著しく少ない場合を除きます）",
            Self::CompatibleCreation => "テクスチャデータ等を作成すること",
            Self::DerivativeWork => "（いわゆる二次創作）を作成すること",
            Self::Credit => "利用時のクレジット表記",
            Self::TransferOfRights => "権利義務の譲渡等",
        }
    }

    fn options(self) -> Options {
        match self {
            Self::PersonalUse | Self::CorporateUse => SUBJECT,
            Self::ThirdPartyUpload => THIRD_PARTY,
            Self::SexualExpression | Self::ViolentExpression => SENSITIVE,
            Self::PoliticalOrReligiousUse => POLITICAL,
            Self::Outsourcing => OUTSOURCING,
            Self::Redistribution | Self::ModifiedDistribution => DISTRIBUTION,
            Self::VideoAndStreaming
            | Self::Publications
            | Self::Merchandise
            | Self::SoftwareEmbedding => MEDIA,
            Self::MeshReuse | Self::CompatibleCreation => REUSE,
            Self::DerivativeWork => DERIVATIVE,
            Self::Credit => CREDIT,
            Self::SocialPlatformUpload
            | Self::GamePlatformUpload
            | Self::Adjustment
            | Self::Modification
            | Self::UseForOtherModels
            | Self::TransferOfRights => BASIC,
        }
    }
}

static FOOTER: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r"この利用規約は、VN3ライセンスVer\.(\d+(?:\.\d+)*)(?:に準拠したVN3ライセンス利用規約ジェネレータVer\.(\d+(?:\.\d+)*)|のテンプレート)を利用して",
    )
    .unwrap()
});
static OFFICIAL_REASON: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^\(公式[^()]{1,8}と誤解されないため\)").unwrap());
static UNKNOWN_STOP: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"[A-X]\.|\(\d+\)|上記の利用の許可には|3\.特記事項").unwrap());
static NOTES_LEAD: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^(?:X\.)?特記事項").unwrap());

const CONDITIONS: &str = "2.利用条件";
const NOTES: &str = "3.特記事項";
const HOLDER: &str = "4.権利者";
const UNKNOWN_RAW_CHARS: usize = 120;
const SECTION_NAMES: [&str; 8] = [
    "利用主体",
    "オンラインサービスへのアップロード",
    "センシティブな表現",
    "加工",
    "再配布・配布",
    "メディア・プロダクトへの利用",
    "二次創作",
    "その他",
];

pub fn parse_vn3(text: &str) -> Option<Vn3License> {
    let text = Text::new(text);
    let footer = FOOTER.captures(&text.compact)?;
    let license_version = footer[1].to_string();
    let origin = match footer.get(2) {
        Some(version) => Vn3Origin::Generator { version: version.as_str().to_string() },
        None => Vn3Origin::Template,
    };

    let mut cursor = text.compact.rfind(CONDITIONS).map(|i| i + CONDITIONS.len());
    let terms = Vn3Item::ALL
        .iter()
        .map(|&item| {
            let anchor = compact(item.anchor());
            let end = cursor.and_then(|from| {
                text.compact[from..].find(&anchor).map(|i| from + i + anchor.len())
            });
            match end {
                Some(end) => {
                    cursor = Some(end);
                    read_answer(&text, item, end)
                }
                None => Vn3Term { item, answer: Vn3Answer::Unknown, raw: String::new() },
            }
        })
        .collect();

    Some(Vn3License { license_version, origin, terms, notes: read_notes(&text) })
}

fn read_answer(text: &Text, item: Vn3Item, start: usize) -> Vn3Term {
    let start = start
        + SECTION_NAMES
            .iter()
            .find(|name| text.compact[start..].starts_with(*name))
            .map_or(0, |name| name.len());
    let rest = &text.compact[start..];
    let matched = item
        .options()
        .iter()
        .filter_map(|&(option, answer)| {
            let option = compact(option);
            rest.starts_with(&option).then_some((answer, option.len()))
        })
        .max_by_key(|&(_, len)| len)
        .map(|(answer, mut len)| {
            if answer == Vn3Answer::AllowIfDistinguishable
                && let Some(reason) = OFFICIAL_REASON.find(&rest[len..])
            {
                len += reason.end();
            }
            (answer, len)
        })
        .filter(|&(_, len)| ends_cleanly(&rest[len..]));

    match matched {
        Some((answer, len)) => {
            Vn3Term { item, answer, raw: text.slice(start, start + len).to_string() }
        }
        None => {
            let stop = UNKNOWN_STOP.find(rest).map_or(rest.len(), |m| m.start());
            let stop = rest[..stop]
                .char_indices()
                .nth(UNKNOWN_RAW_CHARS)
                .map_or(stop, |(i, _)| i);
            Vn3Term {
                item,
                answer: Vn3Answer::Unknown,
                raw: text.slice(start, start + stop).to_string(),
            }
        }
    }
}

fn ends_cleanly(after: &str) -> bool {
    match after.strip_prefix('(') {
        Some(inner) => inner.starts_with(|c: char| c.is_ascii_digit()),
        None => !(after.starts_with("ただし") || after.starts_with(['、', ','])),
    }
}

fn read_notes(text: &Text) -> Option<String> {
    let start = text.compact.rfind(NOTES)? + NOTES.len();
    let start = start + NOTES_LEAD.find(&text.compact[start..]).map_or(0, |m| m.end());
    let end = text.compact[start..].find(HOLDER).map_or(text.compact.len(), |i| start + i);
    let body = &text.compact[start..end];
    if body.is_empty() || body == "なし" || body == "特になし" {
        return None;
    }
    Some(text.slice(start, end).to_string())
}

fn is_invisible(c: char) -> bool {
    matches!(c, '\u{200b}' | '\u{200c}' | '\u{200d}' | '\u{2060}' | '\u{feff}' | '\u{00ad}')
}

fn compact(s: &str) -> String {
    s.chars()
        .filter(|c| !c.is_whitespace() && !is_invisible(*c))
        .flat_map(|c| std::iter::once(c).nfkc())
        .collect()
}

struct Text {
    soft: String,
    compact: String,
    spans: Vec<(usize, usize, usize)>,
}

impl Text {
    fn new(raw: &str) -> Self {
        let mut soft = String::with_capacity(raw.len());
        let mut gap: Option<bool> = None;
        for c in raw.chars().filter(|c| !is_invisible(*c)) {
            if c.is_whitespace() {
                let breaks = matches!(c, '\n' | '\r' | '\u{c}');
                gap = Some(gap.unwrap_or(false) || breaks);
                continue;
            }
            if let Some(breaks) = gap.take()
                && !soft.is_empty()
            {
                soft.push(if breaks { '\n' } else { ' ' });
            }
            soft.push(c);
        }

        let mut compact = String::with_capacity(soft.len());
        let mut spans = Vec::with_capacity(soft.len());
        for (at, c) in soft.char_indices() {
            if c.is_whitespace() {
                continue;
            }
            let end = at + c.len_utf8();
            for n in std::iter::once(c).nfkc() {
                spans.push((compact.len(), at, end));
                compact.push(n);
            }
        }
        Self { soft, compact, spans }
    }

    fn slice(&self, start: usize, end: usize) -> &str {
        if start >= end {
            return "";
        }
        let first = self.spans.partition_point(|&(c, _, _)| c < start);
        let last = self.spans.partition_point(|&(c, _, _)| c < end);
        if first >= last {
            return "";
        }
        self.soft[self.spans[first].1..self.spans[last - 1].2].trim()
    }
}

#[cfg(test)]
mod tests;
