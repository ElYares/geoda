#!/usr/bin/env python3
"""Comprueba que los enlaces relativos de los .md versionados apuntan a archivos que existen.

No sale a la red: ignora http(s), mailto y anclas sueltas. Uso: tests/check_links.py
"""
import re
import subprocess
import sys
from pathlib import Path

raiz = Path(__file__).resolve().parent.parent
archivos = subprocess.run(
    ["git", "ls-files", "*.md"], cwd=raiz, capture_output=True, text=True, check=True
).stdout.split()
enlace = re.compile(r"\[[^\]]*\]\(([^)\s]+)(?:\s+\"[^\"]*\")?\)")
rotos = []
for nombre in archivos:
    origen = raiz / nombre
    en_bloque = False
    for numero, linea in enumerate(origen.read_text(encoding="utf-8").splitlines(), 1):
        if linea.lstrip().startswith("```"):
            en_bloque = not en_bloque
        if en_bloque:
            continue
        for destino in enlace.findall(re.sub(r"`[^`]*`", "", linea)):
            if re.match(r"^(https?:|mailto:|#)", destino):
                continue
            ruta = destino.split("#")[0]
            if ruta and not (origen.parent / ruta).exists():
                rotos.append(f"{nombre}:{numero}: {destino}")
print("\n".join(rotos) or f"{len(archivos)} archivos, sin enlaces rotos")
sys.exit(1 if rotos else 0)
