# Black Gem

Tower defense, automatisation et apprentissage du code (Python puis Rust), écrit en Rust
avec [Bevy](https://bevy.org). Cibles : Windows, Linux, macOS et navigateur (iPhone inclus),
**sans installation**.

- [Game Design Document](docs/GDD.md)

## État actuel : prototype M1

**Objectif : tenir trois nuits.** Chaque nuit, la Corruption sort de ses nids (en rouge, au bord
de la carte) et marche sur le QG.

1. Pose des **foreuses** sur les filons de cristal (violets).
2. Relie-les par des **convoyeurs** au QG (pour gagner des cristaux) ou à des **tours**
   (le cristal sert de munitions : une tour vide ne tire pas).
3. Le QG se défend un peu tout seul, mais il faut au moins quatre tours approvisionnées.

Déjà en place :
- planète 3D procédurale et déterministe (même graine = même planète sur toutes les machines) ;
- courbure « petite planète » pilotée par l'angle de la caméra ;
- foreuses, convoyeurs (tracé en glissant), tours qui pivotent vers leur cible, démolition
  (rembourse 50 %) ;
- vagues nocturnes de plus en plus fortes, victoire / défaite ;
- cycle jour/nuit, ombres, cristaux lumineux (HDR + bloom), brouillard ;
- interface en français ou en anglais (touche **L**), utilisable au clavier, à la souris ou au doigt.

| Touche | Action |
|--------|--------|
| B / C / T | Foreuse / convoyeur / tour |
| X | Démolir |
| R | Tourner le bâtiment |
| Clic gauche (glisser) | Construire (une ligne de convoyeurs) |
| Échap / clic droit | Annuler l'outil |
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
La version web est publiée sur GitHub Pages depuis la branche par défaut du dépôt :
<https://franck-brt.github.io/Black-Gem/>
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
crates/gem_sim/    simulation déterministe, indépendante du moteur : carte, bâtiments,
                   logistique, ennemis, vagues ; pilotée par des ordres (`Command`)
crates/black_gem/  le jeu : rendu Bevy, shaders de courbure, caméra, construction, interface
web/               page web (détection WebGPU/WebGL2, installable sur iPhone)
docs/GDD.md        document de conception du jeu
```

La police de l'interface est [Exo 2](https://github.com/googlefonts/Exo-2.0) (licence SIL
Open Font, voir `crates/black_gem/assets/fonts/OFL.txt`).
