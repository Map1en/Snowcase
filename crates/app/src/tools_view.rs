use gpui_kit::component::*;
use gpui_kit::*;

const TOOLS: [(IconName, &str, &str); 6] = [
    (
        IconName::Frame,
        "Package preview",
        "Open a .unitypackage and browse its files, images and models without Unity.",
    ),
    (
        IconName::HardDrive,
        "Storage cleanup",
        "Find old package versions and duplicates, then archive or remove them.",
    ),
    (
        IconName::FileText,
        "License check",
        "Read VN3 license terms and flag restrictions before you upload or share.",
    ),
    (
        IconName::Globe,
        "Import from BOOTH",
        "Download your BOOTH purchases and fill in item info from the store page.",
    ),
    (
        IconName::Network,
        "Project usage scan",
        "Scan Unity projects to see which assets and versions each one uses.",
    ),
    (
        IconName::Bot,
        "AI assistant",
        "Ask about your assets, shopping, customizing and uploading.",
    ),
];

#[derive(IntoElement)]
pub struct ToolsView;

impl RenderOnce for ToolsView {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let theme = cx.theme();
        v_flex()
            .id("tools")
            .test_support()
            .size_full()
            .overflow_y_scroll()
            .gap_4()
            .p_6()
            .child(div().text_lg().font_semibold().child("Tools"))
            .child(h_flex().flex_wrap().gap_3().children(TOOLS.into_iter().enumerate().map(
                |(ix, (icon, title, text))| {
                    v_flex()
                        .id(("tool", ix))
                        .test_support()
                        .w(px(260.))
                        .gap_2()
                        .p_4()
                        .rounded_lg()
                        .border_1()
                        .border_color(theme.border)
                        .child(
                            h_flex()
                                .gap_2()
                                .items_center()
                                .child(Icon::new(icon).size(px(18.)))
                                .child(div().font_semibold().child(title)),
                        )
                        .child(div().text_sm().text_color(theme.muted_foreground).child(text))
                        .child(div().text_xs().text_color(theme.muted_foreground).child("Planned"))
                },
            )))
    }
}
