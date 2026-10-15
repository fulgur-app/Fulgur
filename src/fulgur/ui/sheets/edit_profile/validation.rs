use super::form_state::ProfileFormState;
use crate::fulgur::Fulgur;
use gpui_kit::{App, Entity, SharedString};

/// Validate a server URL string from the form.
///
/// ### Arguments
/// - `value`: The form value (already trimmed by the caller).
///
/// ### Returns
/// - `Ok(())`: The URL is empty (allowed) or parses successfully.
/// - `Err(SharedString)`: A user-facing error message describing the failure.
pub(super) fn validate_url(value: &str) -> Result<(), SharedString> {
    if value.is_empty() {
        return Ok(());
    }
    url::Url::parse(value)
        .map(|_| ())
        .map_err(|_| SharedString::from("Server URL is not a valid URL."))
}

/// Validate an email string from the form using the same heuristic the
/// settings validator uses (presence of `@` and a `.` after it).
///
/// ### Arguments
/// - `value`: The form value (already trimmed by the caller).
///
/// ### Returns
/// - `Ok(())`: The email is empty (allowed) or passes the heuristic.
/// - `Err(SharedString)`: A user-facing error message describing the failure.
fn validate_email(value: &str) -> Result<(), SharedString> {
    if value.is_empty() {
        return Ok(());
    }
    let at_pos = value.find('@');
    let is_valid = at_pos
        .is_some_and(|pos| pos > 0 && pos < value.len() - 1 && value[pos + 1..].contains('.'));
    if is_valid {
        Ok(())
    } else {
        Err(SharedString::from("Email address is not valid."))
    }
}

/// Check whether the save flow should warn the user about insecure transport.
///
/// ### Arguments
/// - `server_url`: The trimmed server URL string from the form.
///
/// ### Returns
/// - `true`: The URL parses and uses the `http` scheme.
/// - `false`: The URL is empty, invalid, or uses another scheme.
pub(super) fn should_warn_for_http_url(server_url: &str) -> bool {
    if server_url.is_empty() {
        return false;
    }
    url::Url::parse(server_url).is_ok_and(|url| url.scheme().eq_ignore_ascii_case("http"))
}

/// Validate every field in the form, returning the first failure as a user-facing message.
///
/// ### Arguments
/// - `state`: The shared form state.
/// - `entity`: The Fulgur entity (used to check name uniqueness against the
///   current profile list).
/// - `cx`: The application context.
///
/// ### Returns
/// - `Ok(())`: All fields pass validation.
/// - `Err(SharedString)`: A user-facing error message.
pub(super) fn validate_form(
    state: &ProfileFormState,
    entity: &Entity<Fulgur>,
    cx: &App,
) -> Result<(), SharedString> {
    let name = state.name_input.read(cx).value().trim().to_string();
    if name.is_empty() {
        return Err(SharedString::from("Server name cannot be empty."));
    }
    let collides = entity
        .read(cx)
        .settings
        .app_settings
        .synchronization_settings
        .name_collides(&name, Some(&state.profile_id));
    if collides {
        return Err(SharedString::from("Another server already uses this name."));
    }
    let url_value = state.server_url_input.read(cx).value().trim().to_string();
    validate_url(&url_value)?;
    let email_value = state.email_input.read(cx).value().trim().to_string();
    validate_email(&email_value)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{should_warn_for_http_url, validate_email, validate_url};

    #[test]
    fn test_validate_url_accepts_empty() {
        assert!(validate_url("").is_ok());
    }

    #[test]
    fn test_validate_url_rejects_garbage() {
        assert!(validate_url("not-a-url").is_err());
    }

    #[test]
    fn test_validate_url_accepts_https() {
        assert!(validate_url("https://example.com").is_ok());
    }

    #[test]
    fn test_should_warn_for_http_url_accepts_https_without_warning() {
        assert!(!should_warn_for_http_url("https://example.com"));
    }

    #[test]
    fn test_should_warn_for_http_url_warns_for_http() {
        assert!(should_warn_for_http_url("http://example.com"));
    }

    #[test]
    fn test_validate_email_accepts_empty() {
        assert!(validate_email("").is_ok());
    }

    #[test]
    fn test_validate_email_rejects_missing_at() {
        assert!(validate_email("invalid").is_err());
    }

    #[test]
    fn test_validate_email_rejects_missing_dot_after_at() {
        assert!(validate_email("a@b").is_err());
    }

    #[test]
    fn test_validate_email_accepts_simple_address() {
        assert!(validate_email("a@b.c").is_ok());
    }
}

#[cfg(all(test, feature = "gpui-test-support"))]
mod gpui_tests {
    use super::validate_form;
    use crate::fulgur::settings::ServerProfile;
    use crate::fulgur::ui::sheets::edit_profile::form_state::{
        DEVICE_KEY_PLACEHOLDER, gpui_tests::make_form_state,
    };
    use crate::test_support::{open_inert_window, setup_test_globals_with};
    use gpui_kit::TestAppContext;

    const EXISTING_PROFILE_ID: &str = "validate-existing";
    const NEW_PROFILE_ID: &str = "validate-new";

    /// One form submission and the validation outcome it must produce.
    struct Case {
        profile_id: &'static str,
        name: &'static str,
        url: &'static str,
        email: &'static str,
        expected: Result<(), &'static str>,
    }

    /// Describe a submission made from the form of `profile_id`.
    ///
    /// ### Arguments
    /// - `profile_id`: The id of the profile the form edits.
    /// - `name`: The server name typed in the form.
    /// - `url`: The server URL typed in the form.
    /// - `email`: The email address typed in the form.
    /// - `expected`: `Ok(())` or the user-facing error message `validate_form` must return.
    ///
    /// ### Returns
    /// - `Case`: The table row.
    const fn case(
        profile_id: &'static str,
        name: &'static str,
        url: &'static str,
        email: &'static str,
        expected: Result<(), &'static str>,
    ) -> Case {
        Case {
            profile_id,
            name,
            url,
            email,
            expected,
        }
    }

    #[gpui_kit::test]
    fn validate_form_reports_the_first_failing_field(cx: &mut TestAppContext) {
        setup_test_globals_with(cx, |settings| {
            let mut existing = ServerProfile::new("Existing Server");
            existing.id = EXISTING_PROFILE_ID.to_string();
            settings
                .app_settings
                .synchronization_settings
                .profiles
                .push(existing);
        });
        let (fulgur, mut visual_cx) = open_inert_window(cx);
        let empty_name = Err("Server name cannot be empty.");
        let cases = [
            case(NEW_PROFILE_ID, "", "", "", empty_name),
            case(NEW_PROFILE_ID, "   ", "", "", empty_name),
            case(
                NEW_PROFILE_ID,
                " existing server ",
                "",
                "",
                Err("Another server already uses this name."),
            ),
            case(EXISTING_PROFILE_ID, "Existing Server", "", "", Ok(())),
            case(
                NEW_PROFILE_ID,
                "Other",
                "not-a-url",
                "",
                Err("Server URL is not a valid URL."),
            ),
            case(
                NEW_PROFILE_ID,
                "Other",
                "https://example.com",
                "invalid",
                Err("Email address is not valid."),
            ),
            case(
                NEW_PROFILE_ID,
                "Other",
                "https://example.com",
                "user@example.com",
                Ok(()),
            ),
            case(NEW_PROFILE_ID, "Other", "  ", "  ", Ok(())),
        ];
        visual_cx.update(|window, cx| {
            for case in cases {
                let state = make_form_state(
                    case.profile_id,
                    case.profile_id == NEW_PROFILE_ID,
                    case.name,
                    case.url,
                    case.email,
                    DEVICE_KEY_PLACEHOLDER,
                    false,
                    false,
                    window,
                    cx,
                );
                let outcome = validate_form(&state, &fulgur, cx);
                assert_eq!(
                    outcome.map_err(|message| message.to_string()),
                    case.expected.map_err(str::to_string),
                    "name {:?}, url {:?}, email {:?}",
                    case.name,
                    case.url,
                    case.email
                );
            }
        });
    }
}
