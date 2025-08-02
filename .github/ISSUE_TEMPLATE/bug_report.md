---
name: Bug report
about: Create a report to help us improve
title: '[BUG] '
labels: bug
assignees: ''

---

**Describe the bug**
A clear and concise description of what the bug is.

**To Reproduce**
Steps to reproduce the behavior:
1. Run command '...'
2. Call tool '....'
3. With parameters '....'
4. See error

**Expected behavior**
A clear and concise description of what you expected to happen.

**Error Output**
```
Please paste the full error output here
```

**Environment (please complete the following information):**
 - OS: [e.g. macOS, Linux, Windows]
 - Rust version: [e.g. 1.70.0]
 - Dociium version: [e.g. 0.1.0]
 - MCP Client: [e.g. Claude Desktop, other]

**MCP Configuration**
```json
{
  "mcpServers": {
    "dociium": {
      // paste your configuration here
    }
  }
}
```

**Cache Information**
- Cache directory: [e.g. ~/.cache/dociium]
- Cache size: [run `du -sh ~/.cache/dociium` if possible]
- Recently cleared cache: [yes/no]

**Additional context**
Add any other context about the problem here.

**Logs**
If possible, run with debug logging and paste relevant output:
```bash
RUST_LOG=debug dociium
```

**Network connectivity**
- Can you access docs.rs in your browser? [yes/no]
- Are you behind a corporate firewall? [yes/no]
- Any proxy configuration? [describe if applicable]