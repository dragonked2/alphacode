use crate::cli::args::SupportAction;

pub(crate) fn open_support(action: SupportAction) {
    let url = match action {
        SupportAction::Github => "https://github.com/dragonked2/alphacode",
        SupportAction::Donate => "https://buymeacoffee.com/dragonked2",
    };

    if crate::alphacode_base::auth::browser_suppressed(false) || open::that(url).is_err() {
        println!("{url}");
    }
}
