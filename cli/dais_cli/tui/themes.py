"""Theme system for dais TUI."""

from dataclasses import dataclass
from typing import Dict


@dataclass
class Theme:
    """TUI color theme definition."""

    name: str
    description: str

    # Primary colors
    primary: str
    primary_darken_1: str
    primary_darken_2: str
    accent: str

    # Surface colors
    surface: str
    surface_darken_1: str

    # Text colors
    text: str
    text_muted: str

    # Semantic colors
    success: str
    warning: str
    error: str
    info: str

    # Background
    background: str

    def to_css_variables(self) -> str:
        """Convert theme to CSS variable definitions."""
        return f"""
        $primary: {self.primary};
        $primary-darken-1: {self.primary_darken_1};
        $primary-darken-2: {self.primary_darken_2};
        $accent: {self.accent};
        $surface: {self.surface};
        $surface-darken-1: {self.surface_darken_1};
        $text: {self.text};
        $text-muted: {self.text_muted};
        $success: {self.success};
        $warning: {self.warning};
        $error: {self.error};
        $info: {self.info};
        $background: {self.background};
        """


# Built-in themes
THEMES: Dict[str, Theme] = {
    "default": Theme(
        name="Default",
        description="Classic Textual dark theme",
        primary="#004578",
        primary_darken_1="#00335a",
        primary_darken_2="#00243f",
        accent="#0178d4",
        surface="#1e1e1e",
        surface_darken_1="#121212",
        text="#e0e0e0",
        text_muted="#a0a0a0",
        success="#00d787",
        warning="#f4bf75",
        error="#d70000",
        info="#0178d4",
        background="#0e0e0e",
    ),
    "ocean": Theme(
        name="Ocean",
        description="Cool blue tones inspired by the ocean",
        primary="#0d4f8b",
        primary_darken_1="#083558",
        primary_darken_2="#042438",
        accent="#1e90ff",
        surface="#1a1a2e",
        surface_darken_1="#0f0f1e",
        text="#e0e8f0",
        text_muted="#8fa3b8",
        success="#00d4aa",
        warning="#ffb347",
        error="#ff4757",
        info="#1e90ff",
        background="#0a0a14",
    ),
    "forest": Theme(
        name="Forest",
        description="Earthy greens and browns",
        primary="#2d5016",
        primary_darken_1="#1e3610",
        primary_darken_2="#112108",
        accent="#4ecca3",
        surface="#1e2d1a",
        surface_darken_1="#141f12",
        text="#d4e8d4",
        text_muted="#91b891",
        success="#4ecca3",
        warning="#f4a261",
        error="#e63946",
        info="#48cae4",
        background="#0d150a",
    ),
    "sunset": Theme(
        name="Sunset",
        description="Warm oranges and purples",
        primary="#6b2d5c",
        primary_darken_1="#4a1f3f",
        primary_darken_2="#2d1326",
        accent="#ff6b6b",
        surface="#2a1a2e",
        surface_darken_1="#1a0f1e",
        text="#ffe5e5",
        text_muted="#d4a5b8",
        success="#00d4aa",
        warning="#ffb347",
        error="#ff4757",
        info="#c678dd",
        background="#140a14",
    ),
    "nord": Theme(
        name="Nord",
        description="Arctic-inspired palette",
        primary="#5e81ac",
        primary_darken_1="#4c688f",
        primary_darken_2="#3b5166",
        accent="#88c0d0",
        surface="#2e3440",
        surface_darken_1="#242933",
        text="#eceff4",
        text_muted="#d8dee9",
        success="#a3be8c",
        warning="#ebcb8b",
        error="#bf616a",
        info="#81a1c1",
        background="#1e222a",
    ),
    "gruvbox": Theme(
        name="Gruvbox",
        description="Retro warm color scheme",
        primary="#98971a",
        primary_darken_1="#79740e",
        primary_darken_2="#5a5708",
        accent="#b8bb26",
        surface="#282828",
        surface_darken_1="#1d2021",
        text="#ebdbb2",
        text_muted="#a89984",
        success="#b8bb26",
        warning="#fabd2f",
        error="#fb4934",
        info="#83a598",
        background="#1d2021",
    ),
    "solarized-dark": Theme(
        name="Solarized Dark",
        description="Precision colors for machines and people",
        primary="#268bd2",
        primary_darken_1="#1e6fa8",
        primary_darken_2="#16557f",
        accent="#2aa198",
        surface="#002b36",
        surface_darken_1="#001f27",
        text="#fdf6e3",
        text_muted="#93a1a1",
        success="#859900",
        warning="#b58900",
        error="#dc322f",
        info="#268bd2",
        background="#001f27",
    ),
    "monokai": Theme(
        name="Monokai",
        description="Vibrant syntax-inspired theme",
        primary="#66d9ef",
        primary_darken_1="#4db8ca",
        primary_darken_2="#3698a8",
        accent="#a6e22e",
        surface="#272822",
        surface_darken_1="#1e1f1a",
        text="#f8f8f2",
        text_muted="#75715e",
        success="#a6e22e",
        warning="#e6db74",
        error="#f92672",
        info="#66d9ef",
        background="#1e1f1a",
    ),
    "dracula": Theme(
        name="Dracula",
        description="Dark theme with vibrant accents",
        primary="#6272a4",
        primary_darken_1="#4d5b85",
        primary_darken_2="#3a4566",
        accent="#8be9fd",
        surface="#282a36",
        surface_darken_1="#1e2029",
        text="#f8f8f2",
        text_muted="#6272a4",
        success="#50fa7b",
        warning="#f1fa8c",
        error="#ff5555",
        info="#8be9fd",
        background="#1e2029",
    ),
    "light": Theme(
        name="Light",
        description="Light theme for bright environments",
        primary="#0066cc",
        primary_darken_1="#0052a3",
        primary_darken_2="#003d7a",
        accent="#0078d4",
        surface="#f5f5f5",
        surface_darken_1="#e0e0e0",
        text="#1a1a1a",
        text_muted="#666666",
        success="#107c10",
        warning="#ca5010",
        error="#d13438",
        info="#0078d4",
        background="#ffffff",
    ),
}


def get_theme(theme_name: str) -> Theme:
    """Get a theme by name.

    Args:
        theme_name: Name of the theme (case-insensitive)

    Returns:
        Theme object

    Raises:
        KeyError: If theme not found
    """
    return THEMES[theme_name.lower()]


def get_theme_names() -> list[str]:
    """Get list of available theme names.

    Returns:
        List of theme names
    """
    return list(THEMES.keys())


def get_theme_list() -> list[tuple[str, str]]:
    """Get list of themes with descriptions.

    Returns:
        List of (name, description) tuples
    """
    return [(theme.name, theme.description) for theme in THEMES.values()]
