use gpui_kit::component::button::*;
use gpui_kit::component::switch::Switch;
use gpui_kit::component::*;
use gpui_kit::*;

const FEATURES: [(&str, &str); 4] = [
    ("BOOTH library sync", "Download your purchases and keep item info up to date."),
    ("VRCX-0 integration", "Show the assets each of your avatars uses inside VRCX-0."),
    ("License warnings", "Warn about VN3 restrictions when you open an asset."),
    ("AI assistant", "Let AI help with shopping, customizing and uploading."),
];

const LINKS: [(&str, &str); 4] = [
    ("GitHub", "https://github.com/Map1en/Snowcase"),
    ("Report an issue", "https://github.com/Map1en/Snowcase/issues"),
    ("Discord", "https://discord.gg/fehKP3SVPN"),
    ("License (GPL-3.0)", "https://github.com/Map1en/Snowcase/blob/master/LICENSE"),
];

#[derive(IntoElement)]
pub struct SettingsView;

impl RenderOnce for SettingsView {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let theme = cx.theme().clone();
        let dark = theme.is_dark();

        let appearance = section("Appearance", &theme).child(row(
            "Theme",
            "Light or dark interface.",
            ButtonGroup::new("theme")
                .outline()
                .compact()
                .children([
                    Button::new("theme-light").label("Light").selected(!dark),
                    Button::new("theme-dark").label("Dark").selected(dark),
                ])
                .on_click(|selected: &Vec<usize>, window, cx| {
                    let mode = match selected.first() {
                        Some(1) => ThemeMode::Dark,
                        _ => ThemeMode::Light,
                    };
                    Theme::change(mode, Some(window), cx);
                }),
            &theme,
        ));

        let library = section("Library", &theme)
            .child(row(
                "Asset files",
                "Files stay where they are. Snowcase only remembers their location.",
                div(),
                &theme,
            ))
            .child(row(
                "Watched folders",
                "Add new files from these folders automatically. Planned.",
                Button::new("add-watched").outline().compact().label("Add folder").disabled(true),
                &theme,
            ));

        let features = section("Features", &theme).children(FEATURES.into_iter().enumerate().map(
            |(ix, (title, text))| {
                row(
                    title,
                    text,
                    Switch::new(("feature", ix)).checked(false).disabled(true),
                    &theme,
                )
            },
        ));

        let about = section("About", &theme)
            .child(row(
                "Version",
                concat!("Snowcase ", env!("CARGO_PKG_VERSION"), " · early development"),
                div(),
                &theme,
            ))
            .child(h_flex().flex_wrap().gap_2().children(LINKS.into_iter().enumerate().map(
                |(ix, (label, url))| {
                    Button::new(("link", ix))
                        .outline()
                        .compact()
                        .icon(IconName::ExternalLink)
                        .label(label)
                        .on_click(move |_, _, cx| cx.open_url(url))
                },
            )));

        v_flex()
            .id("settings")
            .test_support()
            .size_full()
            .overflow_y_scroll()
            .gap_6()
            .p_6()
            .child(div().text_lg().font_semibold().child("Settings"))
            .child(appearance)
            .child(library)
            .child(features)
            .child(about)
    }
}

fn section(title: &'static str, theme: &Theme) -> Div {
    v_flex()
        .max_w(px(720.))
        .gap_3()
        .child(
            div()
                .pb_1()
                .border_b_1()
                .border_color(theme.border)
                .font_semibold()
                .child(title),
        )
}

fn row(
    title: &'static str,
    text: &'static str,
    control: impl IntoElement,
    theme: &Theme,
) -> Div {
    h_flex()
        .gap_4()
        .justify_between()
        .child(
            v_flex()
                .min_w_0()
                .child(div().text_sm().child(title))
                .child(div().text_xs().text_color(theme.muted_foreground).child(text)),
        )
        .child(control)
}
