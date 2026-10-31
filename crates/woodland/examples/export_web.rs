fn main() -> anyhow::Result<()> {
    woodland::web_package::export(
        std::path::Path::new("assets"),
        std::path::Path::new("web/assets"),
    )
}
