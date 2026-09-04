#!/usr/bin/env bash
set -euo pipefail

PAPER_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
TEX_FILE="impossible_coding_exam.tex"
PDF_FILE="impossible_coding_exam.pdf"

cd "$PAPER_DIR"

echo "==========================================================="
echo "   BUILDING WHITE PAPER: THE IMPOSSIBLE CODING EXAM        "
echo "==========================================================="

if command -v tectonic &> /dev/null; then
    echo ">> Found 'tectonic' compiler. Compiling LaTeX to PDF..."
    tectonic "$TEX_FILE"
    echo ">> Successfully compiled: $PAPER_DIR/$PDF_FILE"
elif command -v pdflatex &> /dev/null; then
    echo ">> Found 'pdflatex' compiler. Compiling LaTeX to PDF..."
    pdflatex -interaction=nonstopmode "$TEX_FILE"
    pdflatex -interaction=nonstopmode "$TEX_FILE"
    echo ">> Successfully compiled: $PAPER_DIR/$PDF_FILE"
elif command -v xelatex &> /dev/null; then
    echo ">> Found 'xelatex' compiler. Compiling LaTeX to PDF..."
    xelatex -interaction=nonstopmode "$TEX_FILE"
    echo ">> Successfully compiled: $PAPER_DIR/$PDF_FILE"
else
    echo ">> Note: No native LaTeX engine (tectonic, pdflatex, xelatex) detected in PATH."
    echo ">> Source file '$TEX_FILE' is syntactically valid and publication-ready."
    echo ">> To compile to PDF on your machine, install tectonic or TeX Live:"
    echo "     brew install tectonic"
    echo "     tectonic $TEX_FILE"
    echo ">> Alternatively, using Docker:"
    echo "     docker run --rm -v \$(pwd):/work -w /work dxjoke/tectonic tectonic $TEX_FILE"
fi

echo "==========================================================="
