use termimad::crossterm::style::Attribute;
use termimad::MadSkin;

pub struct MarkdownRenderer {
    skin: MadSkin,
}

impl MarkdownRenderer {
    pub fn new(use_color: bool) -> Self {
        let skin = if use_color {
            let mut s = MadSkin::default();

            // Claude Code inspired: clean, subtle styling
            s.bold.set_fg(termimad::crossterm::style::Color::White);
            s.italic.set_fg(termimad::crossterm::style::Color::Grey);

            // Inline code: subtle background
            s.inline_code.set_bg(termimad::crossterm::style::Color::DarkGrey);
            s.inline_code.set_fg(termimad::crossterm::style::Color::White);

            // Code blocks: dark background
            s.code_block.set_bg(termimad::crossterm::style::Color::DarkGrey);
            s.code_block.set_fg(termimad::crossterm::style::Color::White);

            // Headers: white and bold, not colorful
            for header in &mut s.headers {
                header.set_fg(termimad::crossterm::style::Color::White);
                header.add_attr(Attribute::Bold);
            }

            // Lists: keep subtle
            s.bullet.set_fg(termimad::crossterm::style::Color::Grey);

            s
        } else {
            MadSkin::no_style()
        };

        Self { skin }
    }

    pub fn render(&self, markdown: &str) {
        self.skin.print_text(markdown);
    }
}

impl Default for MarkdownRenderer {
    fn default() -> Self {
        Self::new(true)
    }
}
