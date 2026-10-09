# Third-party notices

## Windows-MCP UI Automation caching

AUV's Windows accessibility snapshot uses a Rust implementation informed by
CursorTouch/Windows-MCP's UIA property caching and fallback strategy:
https://github.com/CursorTouch/Windows-MCP/blob/b455c2766c63599d466a6178641bac70787979a4/src/windows_mcp/tree/cache_utils.py

Reviewed upstream commit: `b455c2766c63599d466a6178641bac70787979a4`.
The Python package, server and other modules are not bundled. AUV uses its own
bounded control-view traversal and preserves raw strings and empty values.
The upstream license notice is retained below.

MIT License

Copyright (c) 2025 JEOMON GEORGE

Permission is hereby granted, free of charge, to any person obtaining a copy
of this software and associated documentation files (the "Software"), to deal
in the Software without restriction, including without limitation the rights
to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
copies of the Software, and to permit persons to whom the Software is
furnished to do so, subject to the following conditions:

The above copyright notice and this permission notice shall be included in all
copies or substantial portions of the Software.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE
SOFTWARE.