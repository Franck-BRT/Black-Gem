# Black Gem — Game Design Document (v0.1)

> Tower defense + automatisation + apprentissage du code (Python puis Rust).
> Fusion des scénarios **1 (Forge des Cristaux)**, **3 (Planète Rouille)** et des
> **parties courtes du 5 (Convoi)**. Direction artistique inspirée de **Dyson Sphere Program**.

---

## 1. Pitch

L'humanité a découvert les **Gemmes Noires** : des cristaux qui poussent au cœur de
planètes mortes et qui concentrent une énergie colossale. Mais chaque gisement est
gardé par la **Corruption**, un essaim vivant qui dévore tout ce qui approche.

Tu es un **Forgeron-Ingénieur** à bord de **l'Arche**, une station en orbite. À chaque
**expédition**, tu largues une capsule sur une planète générée procéduralement, tu
bâtis une usine, tu forges des gemmes, tu les sertis dans des tours… et tu
**programmes** tes golems-drones pour que tout tourne sans toi. Objectif : extraire
la Gemme Noire du cœur de la planète et repartir en orbite avant d'être submergé.

## 2. Piliers

1. **La gemme est le centre de tout** (GemCraft) : on l'extrait, on la raffine, on la
   fusionne, on la sertit. Elle est ressource, munition et récompense.
2. **L'usine qui se défend** (Factorio / Mindustry) : la production alimente directement
   la défense. Une usine mal pensée = une défense qui s'effondre.
3. **Le code est un super-pouvoir** (7 Billion Humans / The Farmer Was Replaced) : tout
   peut se faire à la main, mais le code permet d'aller 10× plus loin.
4. **Parties courtes, progression longue** (roguelite) : une expédition dure 30–45 min,
   la méta-progression dure des dizaines d'heures.
5. **Beau à regarder tourner** (Dyson Sphere Program) : chaque usine doit être un
   spectacle de lumière, de mouvement et de particules.

## 3. Boucle de jeu

### 3.1 Boucle d'une expédition (30–45 min)

```
Atterrissage ──► Prospection ──► Usine ──► Forge de gemmes ──► Défense
     ▲                                                            │
     │                                                            ▼
  L'Arche ◄── Extraction de la Gemme Noire ◄── Forage du cœur ◄── Vagues
```

1. **Atterrissage** : choix de la zone de largage sur une planète (biome, richesse,
   menace affichés). La capsule devient ton QG.
2. **Prospection** : filons de cristal brut, minerais, sources d'énergie.
3. **Usine** : foreuses → convoyeurs → raffineries → fours de fusion.
4. **Forge** : les cristaux raffinés deviennent des gemmes ; les gemmes fusionnent pour
   monter en grade.
5. **Défense** : tours alimentées en gemmes et en énergie, murs, pièges.
6. **Vagues** : la Corruption attaque par vagues, plus fortes la **nuit** (cycle
   jour/nuit) et à chaque palier de forage.
7. **Forage du cœur** : le grand foreur central progresse par paliers (25 %, 50 %, 75 %,
   100 %), chaque palier déclenche une vague majeure / un boss.
8. **Extraction** : à 100 %, la Gemme Noire est extraite → décollage → retour à l'Arche
   avec le butin. En cas d'échec, on garde une partie des connaissances.

### 3.2 Méta-progression (L'Arche)

- **Arbre de recherche** : nouveaux bâtiments, tours, couleurs de gemmes.
- **Codex des gemmes** : chaque nouvelle combinaison découverte est enregistrée.
- **Bibliothèque de scripts** : tes programmes sont **conservés** d'une partie à l'autre.
  Un bon script écrit une fois = un avantage permanent.
- **Grimoire du code** : les fonctionnalités du langage se débloquent en progressant
  (voir §6). On n'apprend pas tout d'un coup.
- **Carte stellaire** : planètes de difficulté croissante, biomes variés (glace, lave,
  jungle cristalline, désert de rouille, géante gazeuse avec plateformes).

## 4. Système de gemmes

### 4.1 Couleurs (8)

| Couleur  | Effet principal                          | Rôle                |
|----------|------------------------------------------|---------------------|
| Rouge    | Dégâts critiques, brûlure                | Dégâts bruts        |
| Orange   | Vol d'énergie (recharge le réseau)       | Économie            |
| Jaune    | Éclair en chaîne                         | Groupes d'ennemis   |
| Vert     | Poison cumulable                         | Ennemis résistants  |
| Cyan     | Ralentissement, gel                      | Contrôle            |
| Bleu     | Brise-armure                             | Anti-blindés        |
| Violet   | Multi-cibles, rebonds                    | Couverture de zone  |
| **Noire**| Retourne la Corruption contre elle-même  | Fin de partie       |

### 4.2 Grade et fusion

- Deux gemmes de grade *n* → une gemme de grade *n+1* (coût exponentiel, puissance
  plus qu'exponentielle — comme GemCraft).
- Deux couleurs différentes → **gemme hybride** qui combine les effets (dilués).
- La fusion se fait dans des **Fours de fusion** : c'est une chaîne de production, pas
  un clic. Automatiser la fusion des hauts grades est un vrai défi d'ingénierie.

### 4.3 Où vont les gemmes

- **Tours** : la gemme sertie définit l'effet de la tour.
- **Pièges / pylônes** : effets de zone sur les chemins.
- **Bâtiments** : une gemme orange dans une foreuse = bonus de production, etc.
- **Bombes de gemmes** : on peut sacrifier une gemme pour une explosion massive.

## 5. Ennemis : la Corruption

- **Rampants** : nombreux, faibles, suivent les chemins.
- **Blindés** : lents, résistants (→ gemmes bleues).
- **Volants** : ignorent les murs, ciblent les convoyeurs.
- **Dévoreurs** : attaquent les bâtiments de production, pas les tours.
- **Nids** : la Corruption s'étend sur le terrain (à la Creeper World) ; il faut la
  repousser pour accéder à certains filons.
- **Boss de palier** : un par palier de forage, mécaniques uniques.

## 6. Apprentissage du code

### 6.1 Principe

- Les **golems-drones** et les **runes de contrôle** (posées sur des bâtiments)
  exécutent du code.
- Le code s'exécute **pas à pas, visiblement** : la ligne active est surlignée, on voit
  le drone agir, on peut mettre en pause, avancer d'une instruction, inspecter les
  variables.
- Chaque drone a un **budget d'instructions par seconde** (améliorable) : un code
  efficace est récompensé.
- Messages d'erreur **pédagogiques**, en français, avec indice.

### 6.2 Progression Python (Acte I)

1. Appels de fonctions (`miner()`, `aller_vers(cible)`)
2. Variables
3. Conditions (`if` / `else`)
4. Boucles (`while`, `for`)
5. Fonctions (`def`)
6. Listes et dictionnaires
7. Classes simples

### 6.3 Progression Rust (Acte II)

Débloqué quand on atteint les planètes « industrielles » : les machines lourdes
n'acceptent que du Rust (plus rapide = plus d'instructions par seconde).

1. `let` / `let mut`, types de base
2. `match`, `enum`
3. `Option` / `Result` (une foreuse peut renvoyer `None` quand le filon est vide)
4. `struct` / `impl`
5. **Ownership** — mécanique de jeu : une gemme est une *valeur* ; la sertir dans une
   tour la **déplace**, le golem ne la possède plus.
6. **Emprunts** — `&` / `&mut` : plusieurs tours peuvent *lire* un réseau d'énergie,
   une seule peut le *modifier* à la fois.
7. Traits, itérateurs
8. Concurrence (plusieurs golems coordonnés)

### 6.4 Exemple : le même golem, dans les deux langages

```python
# Golem de ravitaillement
while True:
    tour = tour_la_plus_faible()
    if tour.energie < 30:
        gemme = entrepot.prendre("jaune")
        aller_vers(tour)
        tour.sertir(gemme)
    attendre(1)
```

```rust
// Golem de ravitaillement
loop {
    let tour = tour_la_plus_faible();
    if tour.energie < 30 {
        if let Some(gemme) = entrepot.prendre(Couleur::Jaune) {
            aller_vers(&tour);
            tour.sertir(gemme); // la gemme est déplacée : le golem ne la possède plus
        }
    }
    attendre(1);
}
```

### 6.5 Mobile

Sur iPhone, l'éditeur propose un **mode blocs/cartes** (on assemble les instructions
au doigt) qui génère le même code texte. Sur ordinateur, éditeur texte complet avec
coloration et autocomplétion.

## 7. Direction artistique (inspiration Dyson Sphere Program)

- **3D**, caméra orbitale libre (zoom de la vue d'ensemble jusqu'au niveau d'un golem).
- **Planète** : terrain avec **courbure d'horizon simulée par shader** (look « petite
  planète » de DSP, tout en gardant une grille de construction plate et simple).
- **Lumière** : rendu HDR, **bloom** sur les gemmes et les machines, matériaux PBR
  (métal brossé, cristal, verre), ombres dynamiques, cycle jour/nuit.
- **Gemmes** : matériau émissif avec pseudo-réfraction, scintillement, halo coloré.
- **Mouvement** : milliers d'objets sur les convoyeurs (rendu instancié), bras
  robotiques animés, particules d'étincelles, traînées de projectiles.
- **Corruption** : matière noire organique et iridescente qui « respire ».
- **Ciel** : skybox avec la géante gazeuse / l'étoile du système, L'Arche visible en orbite.
- **Préréglages qualité** : Ultra (PC), Moyen (Mac portable), Mobile (iPhone).

## 8. Architecture technique

### 8.1 Choix

| Sujet           | Choix                                                            |
|-----------------|------------------------------------------------------------------|
| Langage         | Rust                                                             |
| Moteur          | Bevy (rendu wgpu : WebGPU / Vulkan / Metal / DX12, ECS)          |
| Langages du jeu | Interpréteur maison : 2 parseurs (sous-ensemble Python et Rust) → même bytecode → même VM |
| Simulation      | Tick fixe, **déterministe**, séparée du rendu (tests, replays)   |
| Sauvegardes     | Fichiers locaux (desktop) / stockage navigateur (web)            |

### 8.2 Organisation du code (workspace Cargo)

```
crates/
  gem_sim/      # Simulation pure : carte, usine, gemmes, ennemis, vagues (sans rendu)
  gem_script/   # Lexer/parseurs Python & Rust, compilateur bytecode, VM sandboxée
  gem_render/   # Rendu Bevy : terrain, shaders, gemmes, particules, caméra
  gem_ui/       # Interfaces : HUD, éditeur de code, codex, Arche
  black_gem/    # Binaire final (desktop + wasm)
```

### 8.3 Plateformes « sans installation »

| Plateforme | Livraison                                                          |
|------------|--------------------------------------------------------------------|
| Windows    | Un `.exe` portable (aucun installeur)                              |
| Linux      | Un binaire portable (+ AppImage éventuelle)                        |
| macOS      | Une `.app` (signature Apple recommandée pour éviter l'alerte Gatekeeper) |
| Web        | WebAssembly + WebGPU (repli WebGL2), jouable dans le navigateur    |
| iPhone/iPad| Version web dans Safari, installable sur l'écran d'accueil (PWA)   |

## 9. Feuille de route

| Jalon | Contenu                                                                  |
|-------|--------------------------------------------------------------------------|
| **M0** | Squelette : workspace, fenêtre Bevy 3D, terrain, caméra, build desktop + web en CI |
| **M1** | Prototype jouable : foreuse → convoyeur → tour, 1 type d'ennemi, vagues |
| **M2** | Gemmes : couleurs, fusion, sertissage, fours de fusion                  |
| **M3** | VM de script + sous-ensemble Python + golems-drones + éditeur          |
| **M4** | Structure d'expédition : génération de planète, forage du cœur, extraction |
| **M5** | L'Arche : méta-progression, recherche, bibliothèque de scripts          |
| **M6** | Passe graphique « DSP » : HDR, bloom, matériaux, particules, jour/nuit  |
| **M7** | Sous-ensemble Rust + Acte II (ownership comme mécanique)                |
| **M8** | Mobile : interface tactile, éditeur par blocs, préréglage qualité       |

## 10. Questions ouvertes

- Langue de l'API du jeu : français (`aller_vers`), anglais (`move_to`), ou les deux ?
- Durée cible d'une expédition : 20, 30–45 ou 60+ minutes ?
- Mode multijoueur coopératif un jour ? (impacte l'architecture déterministe dès M0)
- Narration : histoire scénarisée (campagne) ou purement systémique ?
