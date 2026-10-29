# Black Gem

Tower defense, automatisation et apprentissage du code (Python puis Rust), écrit en Rust
avec [Bevy](https://bevy.org). Cibles : Windows, Linux, macOS et navigateur (iPhone inclus),
**sans installation**.

- [Game Design Document](docs/GDD.md)

## État actuel : prototype M4 — les expéditions

Au lancement, on **choisit son expédition** selon le temps dont on dispose, et la planète
(numéro ou au hasard) :

| Format | Durée | Carte | Journée | Objectif |
|--------|-------|-------|---------|----------|
| Éclaireur | ~15 min | 128 × 128 | 3 min | 2 paliers de forage |
| Expédition | 30–45 min | 192 × 192 | 6 min | 4 paliers |
| Grand forage | 60–90 min | 224 × 224 | 8 min | 6 paliers |
| Colonie | sans limite | 224 × 224 | 6 min | survivre |

**Objectif** : construire le **Grand foreur** (touche **O**) sur le **Cœur** (la flèche
d'obsidienne entourée de violet, touche **V** pour y aller), et l'alimenter en cristaux
(convoyeurs ou golems). Chaque **palier** franchi réveille une vague majeure avec des **boss**.
À 100 %, la **Gemme Noire** sort de terre : il faut tenir jusqu'au **décollage** (1 min).
Chaque nuit, la Corruption attaque plus fort.

**Les golems** sont des drones fabriqués au QG (15 cristaux, panneau en bas à droite). Clic sur
un golem pour ouvrir son **éditeur de code** et lui écrire un programme en **Python** :

```python
# Ravitailleur : apporte des munitions aux tours qui en manquent.
while True:
    for x, y in tours():
        if munitions(x, y) < 12:
            aller_vers(qg())
            if prendre():
                aller_vers(x, y)
                deposer()
    attendre(1)
```

- Les fonctions du jeu existent en **français et en anglais** (`aller_vers` = `move_to`,
  `prendre` = `take`, `tours` = `towers`…) ; les mots-clés restent ceux de Python.
- La **ligne en cours** est surlignée, les **variables** et les messages (`dire` / `print`)
  s'affichent, le mode **pas à pas** exécute une instruction à la fois.
- Les **erreurs** sont expliquées en français ou en anglais, avec la ligne et une suggestion
  (« le nom « stoc » est inconnu — voulais-tu écrire « stock » ? »).
- Quatre **modèles** prêts à l'emploi : Ravitailleur, Sertisseur, Mineur (alimente le Grand
  foreur), Patrouille. Fonctions `coeur()` / `core()` et `forage()` / `drilling()` pour l'objectif.

**Les gemmes** : cinq couleurs de filons (rouge : dégâts, jaune : éclair en chaîne, vert : poison,
cyan : ralentissement, violet : multi-cibles). Taillerie : 3 cristaux → gemme de grade 1. Four :
2 gemmes de même grade → grade supérieur (deux couleurs → gemme hybride). Les gemmes livrées au QG
se sertissent dans les tours (clic sur la gemme, puis sur la tour).

Déjà en place :
- planète 3D procédurale et déterministe (même graine = même planète sur toutes les machines) ;
- courbure « petite planète » pilotée par l'angle de la caméra ;
- foreuses, convoyeurs (tracé en glissant), tailleries, fours, tours, démolition (rembourse 50 %,
  la gemme d'une tour démolie revient dans l'inventaire) ;
- effets des gemmes, rayons colorés, ennemis teintés quand ils sont empoisonnés ou ralentis ;
- golems programmables en Python, éditeur intégré, exécution visible pas à pas ;
- quatre formats d'expédition (15 min à sans limite), Cœur, Grand foreur, paliers et boss,
  extraction de la Gemme Noire et décollage, victoire / défaite ;
- cycle jour/nuit, ombres, cristaux lumineux (HDR + bloom), brouillard ;
- interface en français ou en anglais (touche **L**), utilisable au clavier, à la souris ou au doigt.

| Touche | Action |
|--------|--------|
| B / C / G / F / T | Foreuse / convoyeur / taillerie / four / tour |
| O | Grand foreur (sur le Cœur) |
| V | Aller au Cœur |
| X | Démolir |
| Clic sur une gemme, puis sur une tour | Sertir |
| U | Retirer la gemme d'une tour |
| Clic sur un golem | Ouvrir son programme |
| Échap (dans l'éditeur) | Rendre le clavier au jeu |
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
| N | Nouvelle expédition |
| Entrée (écran de départ) | Atterrir |
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
crates/gem_script/ le langage du jeu : sous-ensemble de Python, compilateur, machine virtuelle
crates/gem_sim/    simulation déterministe, indépendante du moteur : carte, bâtiments,
                   logistique, ennemis, vagues ; pilotée par des ordres (`Command`)
crates/black_gem/  le jeu : rendu Bevy, shaders de courbure, caméra, construction, interface
web/               page web (détection WebGPU/WebGL2, installable sur iPhone)
docs/GDD.md        document de conception du jeu
```

Polices : [Exo 2](https://github.com/googlefonts/Exo-2.0) pour l'interface et
[JetBrains Mono](https://github.com/JetBrains/JetBrainsMono) pour le code (licence SIL Open Font,
voir `crates/black_gem/assets/fonts/`).
