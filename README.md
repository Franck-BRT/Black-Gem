# Black Gem

Tower defense, automatisation et apprentissage du code (Python puis Rust), écrit en Rust
avec [Bevy](https://bevy.org). Cibles : Windows, Linux, macOS et navigateur (iPhone inclus),
**sans installation**.

- [Game Design Document](docs/GDD.md)

## État actuel : prototype M2 — les gemmes

**Objectif : tenir trois nuits.** Chaque nuit, la Corruption sort de ses nids (en rouge, au bord
de la carte) et marche sur le QG. Des tours sans gemme ne suffisent plus : il faut forger.

1. Les filons ont **cinq couleurs**, en grandes régions : rouge (dégâts), jaune (éclair qui
   rebondit), vert (poison), cyan (ralentissement), violet (plusieurs cibles).
2. Une **foreuse** sur un filon produit des cristaux de sa couleur.
3. Une **taillerie** transforme 3 cristaux d'une couleur en une **gemme de grade 1**.
4. Un **four de fusion** fusionne 2 gemmes de même grade en une gemme du grade supérieur ;
   deux couleurs différentes donnent une gemme **hybride** qui cumule leurs effets (dilués).
5. Les gemmes livrées au **QG** rejoignent l'**inventaire** : clic sur une gemme, puis sur une
   tour, pour la sertir. On peut aussi alimenter une tour en gemme directement par convoyeur.
6. Les tours tirent avec des **cristaux** comme munitions (n'importe quelle couleur) : il faut
   toujours les approvisionner.

Déjà en place :
- planète 3D procédurale et déterministe (même graine = même planète sur toutes les machines) ;
- courbure « petite planète » pilotée par l'angle de la caméra ;
- foreuses, convoyeurs (tracé en glissant), tailleries, fours, tours, démolition (rembourse 50 %,
  la gemme d'une tour démolie revient dans l'inventaire) ;
- effets des gemmes, rayons colorés, ennemis teintés quand ils sont empoisonnés ou ralentis ;
- vagues nocturnes de plus en plus fortes, victoire / défaite ;
- cycle jour/nuit, ombres, cristaux lumineux (HDR + bloom), brouillard ;
- interface en français ou en anglais (touche **L**), utilisable au clavier, à la souris ou au doigt.

| Touche | Action |
|--------|--------|
| B / C / G / F / T | Foreuse / convoyeur / taillerie / four / tour |
| X | Démolir |
| Clic sur une gemme, puis sur une tour | Sertir |
| U | Retirer la gemme d'une tour |
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
