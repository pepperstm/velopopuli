#!/bin/zsh
# Doble clic: prepara los datos abiertos de una ruta de VeloPopuli (edificios, árboles,
# postes, tendido eléctrico, ciclovías y agua) para que el mundo 3D se parezca al lugar real.
cd "$(dirname "$0")" || exit 1
if ! command -v uv >/dev/null 2>&1; then
  echo "Falta 'uv'. Instálalo con:  curl -LsSf https://astral.sh/uv/install.sh | sh"
  read -r "?Pulsa Enter para cerrar…"
  exit 1
fi
echo "Rutas en tu biblioteca de VeloPopuli:"
echo
uv run opendata --list
echo
read -r "ruta?Escribe el id de la ruta (la primera columna) y pulsa Enter: "
[ -z "$ruta" ] && exit 0
uv run opendata --route "$ruta"
echo
echo "Listo. Abre VeloPopuli y rueda la ruta: el mundo usará estos datos."
read -r "?Pulsa Enter para cerrar…"
