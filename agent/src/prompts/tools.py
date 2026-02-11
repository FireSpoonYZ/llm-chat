"""Tool description fragments for system prompt composition."""

TOOL_DESCRIPTIONS = {
    "bash": (
        "**bash** - Execute shell commands in the workspace directory.\n"
        "  Usage:\n"
        "  - Use for running programs, installing packages, git operations, "
        "and other terminal tasks.\n"
        "  - DO NOT use bash for file reading (use read), file writing "
        "(use write), file editing (use edit), file searching (use glob), "
        "or content searching (use grep).\n"
        "  - Prefer dedicated tools over bash equivalents: "
        "read instead of cat/head/tail, edit instead of sed/awk, "
        "glob instead of find/ls, grep instead of grep/rg.\n"
        "  - Output exceeding the limit will be truncated. "
        "For large outputs, redirect to a file and use read.\n"
        "  - Avoid long-running or interactive commands "
        "(servers, watchers, editors)."
    ),
    "read": (
        "**read** - Read file contents with line numbers.\n"
        "  Usage:\n"
        "  - Returns content in 'line_number | content' format.\n"
        "  - Supports offset and limit parameters for large files.\n"
        "  - Always read a file before editing it to understand "
        "existing content and structure.\n"
        "  - Can read multiple files in parallel for efficiency.\n"
        "  - Use this instead of bash cat/head/tail."
    ),
    "write": (
        "**write** - Create or overwrite a file with new content.\n"
        "  Usage:\n"
        "  - Parent directories are created automatically.\n"
        "  - Prefer edit over write when modifying existing files — "
        "write replaces the entire file content.\n"
        "  - Always read a file first before overwriting it.\n"
        "  - Use this instead of bash echo/cat with redirection."
    ),
    "edit": (
        "**edit** - Find and replace text in files with precision.\n"
        "  Usage:\n"
        "  - Performs exact string replacement: old_string → new_string.\n"
        "  - old_string must be unique in the file. If not unique, "
        "include more surrounding context to disambiguate, "
        "or use replace_all=true to replace every occurrence.\n"
        "  - Preserve exact indentation (tabs/spaces) from the original.\n"
        "  - Always read the file first to get the exact text to match.\n"
        "  - Use this instead of bash sed/awk for file modifications."
    ),
    "glob": (
        "**glob** - Find files matching a glob pattern.\n"
        "  Usage:\n"
        "  - Supports patterns like '**/*.py', 'src/**/*.ts'.\n"
        "  - Returns matching file paths sorted by modification time.\n"
        "  - Use for locating files by name or extension.\n"
        "  - Use this instead of bash find or ls for file discovery."
    ),
    "grep": (
        "**grep** - Search file contents using regex patterns.\n"
        "  Usage:\n"
        "  - Returns matching lines with file paths and line numbers.\n"
        "  - Supports full regex syntax (e.g., 'log.*Error').\n"
        "  - Can filter by file glob or file type.\n"
        "  - Use for searching code, finding references, "
        "or locating specific content.\n"
        "  - Use this instead of bash grep or rg."
    ),
    "web_fetch": (
        "**web_fetch** - Fetch content from a URL.\n"
        "  Usage:\n"
        "  - HTML is converted to plain text for readability.\n"
        "  - Read-only operation; does not modify any files.\n"
        "  - Use for retrieving documentation, API responses, "
        "or web page content."
    ),
    "web_search": (
        "**web_search** - Search the web using Exa AI.\n"
        "  Usage:\n"
        "  - Returns relevant web page content for a given query.\n"
        "  - Use to find up-to-date information, news, documentation, "
        "or answers to current-events questions.\n"
        "  - Supports 'auto', 'fast', and 'deep' search types.\n"
        "  - Today's date should be included in queries when "
        "searching for recent or time-sensitive information."
    ),
    "code_interpreter": (
        "**code_interpreter** - Execute Python or JavaScript code "
        "in a sandboxed environment.\n"
        "  Usage:\n"
        "  - Use for calculations, data processing, "
        "and quick code experiments.\n"
        "  - Output is captured and returned as text.\n"
        "  - Suitable for tasks that need computation "
        "without modifying workspace files."
    ),
}


def format_tool_descriptions(tool_names: list[str]) -> str:
    """Format tool descriptions for inclusion in the system prompt."""
    parts = ["# Available Tools\n"]
    for name in tool_names:
        desc = TOOL_DESCRIPTIONS.get(name, f"**{name}** - Tool")
        parts.append(f"- {desc}")
    return "\n".join(parts)
