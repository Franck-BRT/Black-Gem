# Black Gem

Tower defense, automatisation et apprentissage du code (Python puis Rust), écrit en Rust
avec [Bevy](https://bevy.org). Cibles : Windows, Linux, macOS et navigateur (iPhone inclus),
**sans installation**.

- [Game Design Document](docs/GDD.md)

## État actuel : prototype M0

- Planète 3D générée procéduralement (plaines, roches, lacs, filons de cristal, minerai),
  déterministe : une même graine donne la même planète sur toutes les machines
  (condition du multijoueur LAN / en ligne).
- Courbure « petite planète » de l'horizon, pilotée par l'angle de la caméra : plate vue de
  dessus (pour construire), arrondie vers l'horizon ou en dézoomant.
- Caméra de stratégie : clavier (ZQSD en AZERTY, WASD en QWERTY), souris, tactile.
- Cycle jour/nuit, ombres du soleil, cristaux lumineux (HDR + bloom), brouillard.
- Interface en français ou en anglais (touche **L**).

| Touche | Action |
|--------|--------|
| ZQSD / WASD / flèches | Déplacer la caméra |
| A / E (AZERTY) – Q / E (QWERTY) | Pivoter |
| Molette / pincement | Zoom |
| Clic droit + glisser / deux doigts | Orienter / incliner |
| H | Retour au QG |
| Espace | Pause |
| 1 / 2 / 3 | Vitesse ×1 / ×5 / ×20 |
| N | Nouvelle planète |
| L | Langue FR / EN |

## Jouer

Chaque passage de la CI (GitHub Actions) produit des exécutables à télécharger et lancer
directement, sans rien installer : `black-gem-windows`, `black-gem-linux`, `black-gem-macos`.
La version web est publiée sur GitHub Pages depuis la branche `main`
(à activer une fois : *Settings → Pages → Source : GitHub Actions*).

> macOS : l'exécutable n'est pas encore signé ; au premier lancement, faire clic droit →
> *Ouvrir*.

## Compiler

```bash
# Linux : dépendances système de Bevy
sudo apt install libasound2-dev libudev-dev libwayland-dev libxkbcommon-dev

cargo run -p black_gem            # jouer
cargo test --workspace            # tests (dont le déterminisme de la simulation)
```

Version web (WebGPU + WebGL2, la page choisit automatiquement) :

```bash
rustup target add wasm32-unknown-unknown
cargo install wasm-bindgen-cli --version 0.2.129   # même version que dans Cargo.lock
./scripts/build-web.sh
python3 -m http.server -d dist/web 8080            # puis ouvrir http://localhost:8080
```

## Organisation

```
crates/gem_sim/    simulation déterministe, indépendante du moteur (carte, temps, hasard)
crates/black_gem/  le jeu : rendu Bevy, shaders de courbure, caméra, interface
web/               page web (détection WebGPU/WebGL2, installable sur iPhone)
docs/GDD.md        document de conception du jeu
```

La police de l'interface est [Exo 2](https://github.com/googlefonts/Exo-2.0) (licence SIL
Open Font, voir `crates/black_gem/assets/fonts/OFL.txt`).
