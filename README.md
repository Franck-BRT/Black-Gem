# Black Gem

Tower defense, automatisation et apprentissage du code (Python puis Rust), écrit en Rust
avec [Bevy](https://bevy.org). Cibles : Windows, Linux, macOS et navigateur (iPhone inclus),
**sans installation**.

- [Game Design Document](docs/GDD.md)

## État actuel : prototype M8 — itérateurs, traits et travail d'équipe

La suite de l'Acte II : le Rust du jeu gagne les outils des « machines lourdes ».

- **Fermetures** `|x| …` (et `move`) : passées à un itérateur, ou rangées dans une variable
  (`let double = |x| x * 2;`) et transmises à une fonction (`f: impl Fn(i64) -> i64`).
- **Itérateurs** : `iter`, `into_iter`, `chars`, `a..b`, puis `map`, `filter`, `filter_map`,
  `enumerate`, `rev`, `take`, `skip`, `take_while`, `skip_while`, `step_by`, `zip`, `copied`,
  `inspect`, et `collect`, `sum`, `count`, `min`, `max`, `min_by_key`, `max_by_key`, `any`,
  `all`, `find`, `position`, `fold`, `for_each`, `last`, `nth`. Comme en vrai Rust, ils sont
  **paresseux** : `find` s'arrête au premier trouvé. Aussi `Option::map`, `and_then`,
  `unwrap_or_else`, `map_or`, `filter`, `ok_or`, et `Vec::retain`, `sort_by_key`.
- **Traits** : `trait Forme { fn aire(&self) -> i64; fn nom(&self) -> String { … } }`,
  `impl Forme for Carre`, méthodes par défaut, fonctions génériques `fn f<T: Forme>(x: &T)` ou
  `x: &impl Forme` (recopiées pour chaque type, comme le fait rustc), `T::nouveau()`, et
  `impl fmt::Display` pour afficher ses types avec `{}`.
- **Golems coordonnés** (Python et Rust) : `reserver(pos)` ne réussit que pour un golem à la
  fois (un verrou, libéré à l'arrêt du programme), `liberer(pos)`, `reservee(pos)` ;
  `envoyer(canal, n)` et `recevoir(canal)` pour se passer des messages ; `mon_numero()`,
  `nb_golems()`. Nouveau modèle **Équipe** : plusieurs golems ravitaillent les tours sans
  jamais servir la même.

```rust
let cible = tours()
    .into_iter()
    .filter(|&t| munitions(t) < 12 && !reservee(t))
    .min_by_key(|&t| munitions(t));
if let Some(tour) = cible {
    if reserver(tour) { /* … livrer … */ liberer(tour); }
}
```

## Passe de confort

- **Les ennemis détruisent ce qui leur barre la route** : un convoyeur ou une tour sur leur
  chemin est attaqué jusqu'à sa destruction (la gemme d'une tour détruite revient dans
  l'inventaire). Un bâtiment qui n'est plus frappé depuis 10 s se répare tout seul.
- **Barres de vie** au-dessus des ennemis blessés, des boss et des bâtiments endommagés.
- **Alertes** au centre de l'écran (« La nuit tombe — vague 3 », « Le QG est attaqué ! »,
  « Tour détruite ! ») et liseré rouge quand le QG est frappé.
- **Sons et musique**, synthétisés par le jeu au démarrage (aucun fichier) : tirs selon la
  gemme, destructions, boss, constructions, alarme de la nuit, victoire et défaite ; une
  nappe calme le jour, une musique plus sombre et pulsée la nuit, en fondu. **M** coupe le son.
- **Tutoriel** de la première expédition : sept étapes qui se cochent toutes seules (foreuse,
  convoyeurs, tour, munitions, golem, première nuit, Grand foreur), avec le bouton utile qui
  clignote. **F1** le réaffiche.

## Prototype M7 — Rust et l'Acte II

Les golems se programment aussi en **Rust**. Un programme Rust est **vérifié avant de
partir**, comme le ferait `rustc` : types, mutabilité (`let mut`), **ownership** et
**emprunts**. Chaque erreur est expliquée en français ou en anglais, avec le code de l'erreur
de `rustc` (E0382…) pour la retrouver dans la documentation officielle. En échange, un golem
Rust exécute **deux fois plus d'instructions** par seconde.

L'ownership devient une mécanique de jeu : un objet porté est une **valeur** de type `Objet`.
`prendre()` la donne au golem, `deposer(objet)` / `sertir(gemme)` la **déplace** dans le
bâtiment, et le compilateur refuse qu'on s'en serve ensuite :

```rust
fn main() {
    loop {
        for tour in tours() {
            if !a_gemme(tour) && gemmes() > 0 {
                aller_vers(qg());
                if let Some(gemme) = prendre_gemme() {
                    let grade = gemme.grade(); // lire n'est pas prendre
                    aller_vers(tour);
                    match sertir(gemme) {
                        Ok(()) => println!("Gemme {grade} sertie"),
                        Err(g) => println!("Refusée : {}", g.grade()),
                    }
                    // gemme.grade() ici : refusé, « gemme » a été déplacée (E0382)
                }
            }
        }
        attendre(2);
    }
}
```

- **Le Rust du jeu** : `let` / `let mut`, entiers, booléens, `&str` et `String`, tuples, `Vec`,
  `if` / `else` (aussi comme valeur), `loop` (avec `break valeur`), `while`, `while let`,
  `for` sur `0..n`, `(a..=b).rev()`, `&v`, `v.iter()`, `v.iter_mut()`, `.enumerate()` ;
  `fn` avec types et `return` ; `match` (motifs, gardes, intervalles, vérification que tous
  les cas sont couverts), `if let`, `let … else` ; `Option` et `Result` (`Some`, `None`,
  `Ok`, `Err`, `?`, `unwrap`, `expect`, `unwrap_or`…) ; `struct`, `enum` (avec données),
  `impl` et méthodes (`&self`, `&mut self`, `self`), `#[derive(Debug, Clone, Copy, PartialEq)]` ;
  `const` ; `println!`, `format!` (`{}`, `{:?}`, `{nom}`), `panic!`, `assert!`, `vec!`.
- **Vérifications** : types (avec déduction), valeurs déplacées (E0382, y compris d'un tour de
  boucle à l'autre), variables non initialisées (E0381), mutabilité (E0384, E0596), emprunts
  en conflit dans un appel (E0499, E0502), déplacement hors d'une référence ou d'un `Vec`
  (E0507), `match` incomplet (E0004), affichage sans `Debug` (E0277)…
- Les fonctions du jeu ont des **types** : `tours() -> Vec<Position>`,
  `ennemi_proche() -> Option<Position>`, `deposer(objet: Objet) -> Result<(), Objet>`,
  `avancer(Direction::Nord)`… en français ou en anglais.
- L'éditeur propose les quatre **modèles en Rust** (bouton « Python / Rust » devant les
  modèles), indente après `{` et désindente sur `}`. L'inspecteur affiche les valeurs à la
  manière de Rust (`Some((3, 4))`, `Objet { est_gemme: true, grade: 2 }`).
- L'Arche a un **Grimoire** : les deux actes de l'apprentissage et les règles de l'ownership.

Simplifications, signalées par des messages clairs : tous les entiers sont des `i64` ; une
référence `&mut` se passe en argument mais ne se garde pas dans une variable ; pas de
`dyn Trait` ni de structures génériques.

### Passe graphique (M6)

- **Ciel** : dôme dégradé jour/nuit, **géante gazeuse** à bandes au-dessus de l'horizon, soleil
  avec halo, **étoiles** la nuit.
- **Eau** animée : vagues, reflets et effet de Fresnel (shader dédié, courbure comprise).
- **Particules** : étincelles des foreuses et des impacts (couleur de la gemme), éclats de
  Corruption à la mort des ennemis (gros éclats pour les boss), étincelles des fours,
  poussière du Grand foreur, flammes des golems.
- **Atterrissage** : la capsule descend du ciel au début de chaque expédition, caméra en
  plan large qui se rapproche.
- Relief ombré (creux et pentes), **chevrons animés** sur les convoyeurs (vitesse réelle des
  objets), **grille de construction** autour du curseur.
- Sur ordinateur : occlusion ambiante (SSAO) et anticrénelage FXAA.

### L'Arche (M5)

**L'Arche** est la station en orbite entre deux expéditions. Elle garde la progression
(sauvegarde automatique : un fichier `BlackGem/profile.json` dans le dossier de données de
l'utilisateur sur ordinateur, le stockage local du navigateur sur le web et l'iPhone) :

- **Éclats noirs** : gagnés à la fin de chaque expédition (victoire : 15 / 40 / 90 selon le
  format + 2 par nuit ; échec : 5 par palier franchi + 2 par nuit ; Colonie : 5 par nuit).
- **Recherche** : 12 améliorations permanentes avec prérequis (réserves de départ, blindage du
  QG, foreuses et convoyeurs rapides, tours renforcées, optique de visée, taille efficace,
  golems véloces et économiques, trépan du Cœur, gemme de départ).
- **Codex** : les 31 combinaisons de couleurs de gemmes, découvertes ou non, avec le meilleur
  grade atteint.
- **Bibliothèque** : bouton « Enregistrer » dans l'éditeur ; les programmes enregistrés se
  rechargent en un clic dans toutes les expéditions suivantes.
- **Statistiques** : expéditions, victoires, ennemis détruits, meilleurs temps par format.

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
| M | Couper / remettre le son |
| F1 | Tutoriel |

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
crates/gem_script/ les langages du jeu : sous-ensembles de Python et de Rust (avec vérification
                   des types et de l'ownership), compilateur, machine virtuelle
crates/gem_sim/    simulation déterministe, indépendante du moteur : carte, bâtiments,
                   logistique, ennemis, vagues ; pilotée par des ordres (`Command`)
crates/black_gem/  le jeu : rendu Bevy, shaders de courbure, caméra, construction, interface
web/               page web (détection WebGPU/WebGL2, installable sur iPhone)
docs/GDD.md        document de conception du jeu
```

Polices : [Exo 2](https://github.com/googlefonts/Exo-2.0) pour l'interface et
[JetBrains Mono](https://github.com/JetBrains/JetBrainsMono) pour le code (licence SIL Open Font,
voir `crates/black_gem/assets/fonts/`).
