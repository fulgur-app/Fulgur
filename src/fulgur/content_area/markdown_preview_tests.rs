use crate::fulgur::editor_tab::replace_editor_text;
use crate::fulgur::languages::supported_languages::SupportedLanguage;
use crate::fulgur::settings::MarkdownPreviewMode;
use crate::fulgur::shared_state::SharedAppState;
use crate::test_support::{open_rendered_window, setup_test_globals_with};
use core::prelude::v1::test;
use gpui_kit::base::text::TextView;
use gpui_kit::component::text::{FrontmatterPlugin, MarkdownExtensions, TextViewState};
use gpui_kit::http_client::{AsyncBody, FakeHttpClient, Response};
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::{
    AppContext, BorrowAppContext, Render, TestAppContext, VisualTestContext, WindowOptions,
};
use gpui_kit::{Context, Entity, IntoElement, Window};
use std::sync::{Arc, Mutex};

/// Verify the real preview renderer blocks network requests until opted in.
///
/// ### Arguments
/// - `mode`: The preview presentation to exercise.
/// - `cx`: The test application context.
fn check_remote_image_permission(mode: &MarkdownPreviewMode, cx: &mut TestAppContext) {
    let urls = [
        "https://example.com/pixel.png",
        "http://127.0.0.1/local.png",
        "http://192.168.1.1/lan.png",
    ];
    let requests = Arc::new(Mutex::new(Vec::new()));
    let captured = requests.clone();
    cx.update(|cx| {
        cx.set_http_client(FakeHttpClient::create(move |req| {
            captured.lock().unwrap().push(req.uri().to_string());
            async {
                Ok(Response::builder()
                    .status(404)
                    .body(AsyncBody::default())
                    .unwrap())
            }
        }));
    });
    setup_test_globals_with(cx, |settings| {
        settings.editor_settings.markdown_settings.preview_mode = mode.clone();
    });
    let (fulgur, _, mut visual_cx) = open_rendered_window(cx);
    let source = format!(
        "![tracking]({0})\n\n![local]({1})\n\n![reference][ref]\n\n[ref]: {1}\n\n<img src=\"{2}\">",
        urls[0], urls[1], urls[2]
    );
    let content = visual_cx.update(|window, cx| {
        fulgur.update(cx, |this, cx| {
            this.update_active_editor_tab(cx, |editor, cx| {
                editor.language = SupportedLanguage::Markdown;
                editor.show_markdown_preview = true;
                editor.content.update(cx, |content, cx| {
                    replace_editor_text(content, &source, window, cx);
                });
            });
            let content = this
                .get_active_editor_tab(cx)
                .expect("source editor")
                .content
                .clone();
            if *mode == MarkdownPreviewMode::DedicatedTab {
                this.open_markdown_preview_tab(window, cx);
            }
            cx.notify();
            content
        })
    });
    visual_cx.run_until_parked();
    assert!(
        requests.lock().unwrap().is_empty(),
        "preview fetched a blocked image"
    );

    // Exercise the same global settings observer used by every window,
    // without persisting test settings to the user's configuration.
    visual_cx.update(|_window, cx| {
        cx.update_global::<SharedAppState, _>(|shared, _| {
            shared
                .settings
                .editor_settings
                .markdown_settings
                .load_remote_images = true;
        });
    });
    visual_cx.run_until_parked();
    {
        let fetched = requests.lock().unwrap();
        for url in urls {
            assert!(
                fetched.iter().any(|request| request == url),
                "missing opted-in request for {url}: {fetched:?}"
            );
        }
    }
    requests.lock().unwrap().clear();
    visual_cx.update(|_window, cx| {
        cx.update_global::<SharedAppState, _>(|shared, _| {
            shared
                .settings
                .editor_settings
                .markdown_settings
                .load_remote_images = false;
        });
    });
    visual_cx.run_until_parked();
    visual_cx.update(|window, cx| {
        content.update(cx, |content, cx| {
            replace_editor_text(content, &source.replace(".png", "-new.png"), window, cx);
        });
    });
    visual_cx.run_until_parked();
    assert!(
        requests.lock().unwrap().is_empty(),
        "preview fetched an image after permission was revoked"
    );
}

#[gpui_kit::test]
fn preview_tab_requires_remote_image_permission(cx: &mut TestAppContext) {
    check_remote_image_permission(&MarkdownPreviewMode::DedicatedTab, cx);
}

#[gpui_kit::test]
fn preview_panel_requires_remote_image_permission(cx: &mut TestAppContext) {
    check_remote_image_permission(&MarkdownPreviewMode::Panel, cx);
}

/// A window root holding nothing but the preview under test.
struct PreviewView {
    state: Entity<TextViewState>,
    frontmatter: bool,
}

impl Render for PreviewView {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        TextView::new(&self.state)
            .selectable(true)
            .when(self.frontmatter, |view| {
                view.markdown_extensions(
                    MarkdownExtensions::default()
                        .frontmatter()
                        .plugin(FrontmatterPlugin::new()),
                )
            })
    }
}

/// Draw `source` as a Markdown preview and return its rendered plain text.
///
/// ### Arguments
/// - `source`: The raw Markdown to render
/// - `frontmatter`: Whether to enable the frontmatter extension and plugin
/// - `cx`: The test application context
///
/// ### Returns
/// - `String`: The text the renderer produced for the parsed document
fn render_preview(source: &str, frontmatter: bool, cx: &mut TestAppContext) -> String {
    cx.update(gpui_kit::init);
    let state_slot: std::cell::RefCell<Option<Entity<TextViewState>>> =
        std::cell::RefCell::new(None);
    let window = cx
        .update(|cx| {
            cx.open_window(WindowOptions::default(), |_window, cx| {
                let state = cx.new(|cx| TextViewState::markdown(source, cx));
                *state_slot.borrow_mut() = Some(state.clone());
                cx.new(|_| PreviewView { state, frontmatter })
            })
        })
        .expect("failed to open the preview test window");
    let visual_cx = VisualTestContext::from_window(window.into(), cx);
    visual_cx.run_until_parked();
    let state = state_slot
        .into_inner()
        .expect("failed to capture the preview state");
    state.update(cx, |state, cx| {
        state.select_all(cx);
    });
    cx.update(|cx| state.read(cx).selected_text())
}

#[gpui_kit::test]
fn renders_a_soft_wrapped_paragraph(cx: &mut TestAppContext) {
    let text = render_preview("line one\nline two\nline three\n", false, cx);
    assert!(
        text.contains("line one line two line three"),
        "got {text:?}"
    );
}

#[gpui_kit::test]
fn renders_inline_br_tags(cx: &mut TestAppContext) {
    let text = render_preview("a<br>b\n\na<br />b\n", false, cx);
    assert!(text.contains('a'), "got {text:?}");
}

#[gpui_kit::test]
fn renders_a_multiline_html_block(cx: &mut TestAppContext) {
    render_preview(
        "<details>\n<summary>Build</summary>\n\n### Steps\n",
        false,
        cx,
    );
}

#[gpui_kit::test]
fn renders_a_badge_block(cx: &mut TestAppContext) {
    render_preview("[![A](a)](x)\n[![B](b)](y)\n", false, cx);
}

#[gpui_kit::test]
fn renders_hard_breaks_nested_lists_and_fenced_code(cx: &mut TestAppContext) {
    let text = render_preview(
        "Owner: Jane  \nPersona: assistant\n\n- one\n  - nested\n\n```html\n<br>\n<div>\n```\n",
        false,
        cx,
    );
    assert!(text.contains("nested"), "got {text:?}");
}

#[gpui_kit::test]
fn renders_a_table(cx: &mut TestAppContext) {
    let text = render_preview("| a | b |\n|---|---|\n| 1 | 2 |\n", false, cx);
    assert!(text.contains('a'), "got {text:?}");
}

#[gpui_kit::test]
fn keeps_frontmatter_as_a_code_block_when_disabled(cx: &mut TestAppContext) {
    let text = render_preview("---\ntitle: Notes\n---\n\nBody\n", false, cx);
    assert!(text.contains("Body"), "got {text:?}");
}

#[gpui_kit::test]
fn renders_frontmatter_entries_when_enabled(cx: &mut TestAppContext) {
    let text = render_preview("---\ntitle: Notes\nowner: Jane\n---\n\nBody\n", true, cx);
    assert!(text.contains("title"), "got {text:?}");
    assert!(text.contains("Notes"), "got {text:?}");
}

/// Malformed and non-mapping frontmatter must fall back without aborting.
#[gpui_kit::test]
fn tolerates_non_mapping_frontmatter(cx: &mut TestAppContext) {
    render_preview("---\n- just\n- a list\n---\n\nBody\n", true, cx);
    render_preview("---\nnot: [closed\n", true, cx);
}
