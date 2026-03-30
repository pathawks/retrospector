pub trait Title {
    fn title(&self) -> &str;
}

impl std::fmt::Display for dyn Title {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        writeln!(f, "Game Title: {}", &self.title())
    }
}
