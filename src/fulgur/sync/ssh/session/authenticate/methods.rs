/// Authentication methods the server accepts for a user.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct AllowedMethods {
    pub(super) publickey: bool,
    pub(super) password: bool,
    pub(super) keyboard_interactive: bool,
}

impl AllowedMethods {
    /// Assumed methods when the server does not report its list.
    pub(super) const ALL: Self = Self {
        publickey: true,
        password: true,
        keyboard_interactive: false,
    };

    /// Parse the comma-separated method list returned by `libssh2_userauth_list`.
    ///
    /// ### Arguments
    /// - `list`: Method names, e.g. `"publickey,password"`.
    ///
    /// ### Returns
    /// - `AllowedMethods`: Flags for the methods Fulgur supports.
    pub(super) fn parse(list: &str) -> Self {
        let mut methods = Self {
            publickey: false,
            password: false,
            keyboard_interactive: false,
        };
        for method in list.split(',').map(str::trim) {
            match method {
                "publickey" => methods.publickey = true,
                "password" => methods.password = true,
                "keyboard-interactive" => methods.keyboard_interactive = true,
                _ => {}
            }
        }
        methods
    }

    /// Report whether a password typed by the user can be sent to the server.
    ///
    /// ### Returns
    /// - `true`: The server accepts `password` or `keyboard-interactive`.
    /// - `false`: Neither method is accepted.
    pub(super) fn accepts_password(self) -> bool {
        self.password || self.keyboard_interactive
    }
}

#[cfg(test)]
mod tests {
    use super::AllowedMethods;

    #[test]
    fn allowed_methods_parse_reads_supported_methods() {
        assert_eq!(
            AllowedMethods::parse("publickey,password"),
            AllowedMethods::ALL
        );
        let keyboard_only = AllowedMethods::parse("publickey,keyboard-interactive");
        assert_eq!(
            keyboard_only,
            AllowedMethods {
                publickey: true,
                password: false,
                keyboard_interactive: true,
            }
        );
        assert!(keyboard_only.accepts_password());
        let nothing = AllowedMethods::parse("");
        assert!(!nothing.publickey);
        assert!(!nothing.accepts_password());
    }
}
