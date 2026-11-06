/// A server-rendered page that matches the sign-in screen. The worker sends
/// it without the app, so it links the same stylesheet and has no inline style.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Page {
    Denied,
    NotConfigured,
    Failed,
}

impl Page {
    pub fn status(self) -> u16 {
        match self {
            Self::Denied => 403,
            Self::NotConfigured | Self::Failed => 500,
        }
    }

    pub fn html(self) -> String {
        let (title, heading, body) = match self {
            Self::Denied => (
                "Not allowed",
                "This Radar is not yours",
                "Radar lets one GitHub account in, and you signed in with another. \
                 Nothing was saved. To switch accounts, sign out of GitHub first.",
            ),
            Self::NotConfigured => (
                "Sign-in is not set up",
                "Sign-in is not set up",
                "Radar is missing a setting it needs to sign you in. \
                 The Worker log names the setting.",
            ),
            Self::Failed => (
                "Sign-in failed",
                "Sign-in failed",
                "Radar could not finish signing you in. Try again in a minute.",
            ),
        };
        format!(
            "<!doctype html>\n\
             <html lang=\"en\">\n\
             <head>\n\
             <meta charset=\"utf-8\">\n\
             <meta name=\"viewport\" content=\"width=device-width, initial-scale=1\">\n\
             <meta name=\"color-scheme\" content=\"dark\">\n\
             <meta name=\"robots\" content=\"noindex\">\n\
             <title>{title} · Radar</title>\n\
             <link rel=\"icon\" href=\"/icon.svg\" type=\"image/svg+xml\">\n\
             <link rel=\"stylesheet\" href=\"/fonts/fonts.css\">\n\
             <link rel=\"stylesheet\" href=\"/app.css\">\n\
             </head>\n\
             <body>\n\
             <main class=\"auth\">\n\
             <img class=\"auth-mark\" src=\"/icon.svg\" alt=\"\" width=\"40\" height=\"40\">\n\
             <h1 class=\"auth-title\">{heading}</h1>\n\
             <p class=\"auth-lede\">{body}</p>\n\
             <a class=\"auth-button\" href=\"/signin\">Back to sign-in</a>\n\
             </main>\n\
             </body>\n\
             </html>\n"
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const ALL: [Page; 3] = [Page::Denied, Page::NotConfigured, Page::Failed];

    #[test]
    fn denied_is_a_403_and_the_rest_are_500() {
        assert_eq!(Page::Denied.status(), 403);
        assert_eq!(Page::NotConfigured.status(), 500);
        assert_eq!(Page::Failed.status(), 500);
    }

    #[test]
    fn pages_are_dark_documents_without_inline_style_or_script() {
        for page in ALL {
            let html = page.html();
            assert!(html.starts_with("<!doctype html>"), "{page:?}");
            assert!(
                html.contains("<meta name=\"color-scheme\" content=\"dark\">"),
                "{page:?}"
            );
            for banned in ["<style", "style=", "<script", "javascript:"] {
                assert!(!html.contains(banned), "{page:?} contains {banned}");
            }
            assert!(html.contains("href=\"/app.css\""), "{page:?}");
        }
    }
}
