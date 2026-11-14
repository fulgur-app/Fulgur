use data_url::DataUrl;
use gpui_kit::http_client::Url;
use gpui_kit::{Image, ImageFormat, ImageSource, SharedUri};
use std::sync::{Arc, LazyLock};

/// An embedded placeholder that cannot trigger filesystem or network access.
static BLOCKED_IMAGE: LazyLock<Arc<Image>> = LazyLock::new(|| {
    Arc::new(Image::from_bytes(
        ImageFormat::Svg,
        br##"<svg xmlns="http://www.w3.org/2000/svg" width="180" height="24"><rect width="180" height="24" rx="4" fill="#eee"/><text x="8" y="16" font-size="12" fill="#555">Remote image blocked</text></svg>"##.to_vec(),
    ))
});

/// Resolve a preview image without fetching remote content unless opted in.
///
/// ### Arguments
/// - `uri`: The image URI supplied by the document.
/// - `load_remote_images`: Whether the user has enabled HTTP and HTTPS images.
///
/// ### Returns
/// - `ImageSource`: A local or permitted remote URI, decoded embedded image,
///   or an embedded placeholder for blocked or unsupported URIs.
#[must_use]
pub fn preview_image_source(uri: &SharedUri, load_remote_images: bool) -> ImageSource {
    if let Ok(data) = DataUrl::process(uri)
        && let Some(format) = ImageFormat::from_mime_type(&format!(
            "{}/{}",
            data.mime_type().type_,
            data.mime_type().subtype
        ))
        && let Ok((bytes, _fragment)) = data.decode_to_vec()
    {
        return ImageSource::Image(Arc::new(Image::from_bytes(format, bytes)));
    }
    if let Ok(url) = Url::parse(uri)
        && (url.scheme() == "file"
            || (load_remote_images && matches!(url.scheme(), "http" | "https")))
    {
        return uri.clone().into();
    }
    ImageSource::Image(Arc::clone(&BLOCKED_IMAGE))
}

#[cfg(test)]
mod tests {
    use super::super::test_helpers::expected_url;
    use super::{BLOCKED_IMAGE, preview_image_source};
    use gpui_kit::{ImageSource, Resource};
    use std::sync::Arc;

    #[test]
    fn preview_blocks_remote_and_unsupported_image_sources_by_default() {
        for uri in [
            "https://example.com/pixel.png",
            "http://localhost/admin",
            "http://127.0.0.1/pixel.png",
            "http://192.168.1.1/pixel.png",
            "http://10.0.0.1/pixel.png",
            "http://172.16.0.1/pixel.png",
            "http://169.254.169.254/latest/meta-data/",
            "http://[::1]/pixel.png",
            "HTTP://LOCALHOST/pixel.png",
            "//example.com/pixel.png",
            "ftp://example.com/pixel.png",
        ] {
            let ImageSource::Image(image) = preview_image_source(&uri.into(), false) else {
                panic!("expected a non-network placeholder for {uri}");
            };
            assert!(Arc::ptr_eq(&image, &BLOCKED_IMAGE), "{uri}");
        }
    }

    #[test]
    fn preview_remote_images_require_explicit_opt_in() {
        for uri in ["http://localhost/a.png", "https://example.com/a.png"] {
            assert!(matches!(
                preview_image_source(&uri.into(), true),
                ImageSource::Resource(Resource::Uri(url)) if url.as_ref() == uri
            ));
        }
    }

    #[test]
    fn preview_keeps_local_images_uri_backed() {
        let uri = expected_url("image.png");
        for enabled in [false, true] {
            assert!(matches!(
                preview_image_source(&uri.clone().into(), enabled),
                ImageSource::Resource(Resource::Uri(url)) if url.as_ref() == uri
            ));
        }
    }

    #[test]
    fn preview_decodes_embedded_images_without_remote_permission() {
        for (uri, expected) in [
            (
                "data:image/png;base64,iVBORw0KGgo=",
                b"\x89PNG\r\n\x1a\n".as_slice(),
            ),
            (
                "data:image/svg+xml,%3Csvg%3E%3C/svg%3E",
                b"<svg></svg>".as_slice(),
            ),
        ] {
            let ImageSource::Image(image) = preview_image_source(&uri.into(), false) else {
                panic!("expected an embedded image for {uri}");
            };
            assert_eq!(image.bytes(), expected);
        }
    }
}
