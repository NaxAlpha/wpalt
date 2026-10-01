# Third-party notices

The bundled admin studio uses Preact 11.0.0. Its notice is retained in the generated JavaScript and distribution archive. Build-only tools are locked in `frontend/package-lock.json`; Rust dependencies are locked in `Cargo.lock`. wpalt itself has no selected public-adoption license yet.

## Preact

The MIT License (MIT)

Copyright (c) 2015-present Jason Miller

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

Development-only visual verification uses pixelmatch 7.2.0 and pngjs 7.0.0 (MIT), and axe-core 4.13.0 (MPL-2.0), through the locked npm toolchain. These scanners/diff libraries are not included in the application runtime bundle. Their package notices remain in the installed development dependencies.
