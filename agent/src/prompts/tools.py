"""Tool description fragments for system prompt composition."""

TOOL_DESCRIPTIONS = {
    "bash": (
        "**bash** - Execute shell commands in the workspace directory. "
        "Use for running programs, installing packages, or filesystem operations."
    ),
    "read": (
        "**read** - Read file contents with line numbers. "
        "Supports offset and limit for large files."
    ),
    "write": (
        "**write** - Create or overwrite files. "
        "Parent directories are created automatically."
    ),
    "edit": (
        "**edit** - Find and replace text in files. "
        "By default requires exactly one match; use replace_all for multiple."
    ),
    "glob": (
        "**glob** - Find files matching a glob pattern (e.g., '**/*.py')."
    ),
    "grep": (
        "**grep** - Search file contents with regex patterns. "
        "Returns matching lines with file paths and line numbers."
    ),
    "web_fetch": (
        "**web_fetch** - Fetch content from a URL. "
        "HTML is converted to plain text."
    ),
    "code_interpreter": (
        "**code_interpreter** - Execute Python or JavaScript code "
        "and return the output."
    ),
}


def format_tool_descriptions(tool_names: list[str]) -> str:
    """Format tool descriptions for inclusion in the system prompt."""
    parts = ["# Available Tools\n"]
    for name in tool_names:
        desc = TOOL_DESCRIPTIONS.get(name, f"**{name}** - Tool")
        parts.append(f"- {desc}")
    return "\n".join(parts)
