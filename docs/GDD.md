# Black Gem — Game Design Document (v0.2)

> Tower defense + automatisation + apprentissage du code (Python puis Rust).
> Fusion des scénarios **1 (Forge des Cristaux)**, **3 (Planète Rouille)** et des
> **parties courtes du 5 (Convoi)**, avec des systèmes de **RTS classiques** (Red Alert, Warcraft).
> Direction artistique inspirée de **Dyson Sphere Program**.

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
4. **Parties à la carte, progression longue** (roguelite) : le joueur choisit une
   expédition selon le temps dont il dispose, de 15 min à plus d'une heure ; la
   méta-progression dure des dizaines d'heures.
5. **Beau à regarder tourner** (Dyson Sphere Program) : chaque usine doit être un
   spectacle de lumière, de mouvement et de particules.
6. **L'âme des RTS d'antan** (Red Alert, Warcraft) : base, énergie, arbre technologique,
   héros, armées, escarmouches, parties en réseau local.

## 3. Boucle de jeu

### 3.1 Boucle d'une expédition

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

### 3.2 Durée des expéditions

Plusieurs formats sont **toujours** proposés sur la carte stellaire, pour que le joueur
choisisse selon sa disponibilité :

| Format      | Durée     | Contenu                                                     |
|-------------|-----------|-------------------------------------------------------------|
| Éclaireur   | ~15 min   | Petite carte, 2 paliers de forage, pas de cycle nuit complet |
| Expédition  | 30–45 min | Format standard, 4 paliers, 1 à 2 nuits                     |
| Grand forage| 60–90 min | Grande carte, 6 paliers, plusieurs boss, usine complexe     |
| Colonie     | sans limite | Bac à sable : on reste tant qu'on survit, sauvegarde libre |

La durée change la taille de la carte, le nombre de paliers et la longueur du cycle
jour/nuit, mais pas les règles : ce qu'on apprend dans un format sert dans tous les autres.
Une expédition peut être sauvegardée et reprise plus tard.

### 3.3 Méta-progression (L'Arche)

- **Arbre de recherche** : nouveaux bâtiments, tours, couleurs de gemmes.
- **Codex des gemmes** : chaque nouvelle combinaison découverte est enregistrée.
- **Bibliothèque de scripts** : tes programmes sont **conservés** d'une partie à l'autre.
  Un bon script écrit une fois = un avantage permanent.
- **Grimoire du code** : les fonctionnalités du langage se débloquent en progressant
  (voir §7). On n'apprend pas tout d'un coup.
- **Carte stellaire** : planètes de difficulté croissante, biomes variés (glace, lave,
  jungle cristalline, désert de rouille, géante gazeuse avec plateformes).

### 3.4 Histoire et modes de jeu

- **Campagne** : une histoire scénarisée, avec des briefings de mission à l'ancienne
  (portrait animé, carte tactique, objectifs) entre les expéditions. Elle introduit
  progressivement les mécaniques et le code.
- **Expéditions libres** : génération procédurale, méta-progression roguelite.
- **Escarmouche** : une carte, des adversaires (IA ou joueurs), réglages libres.
- **Défis de code** : puzzles courts à la Human Resource Machine, avec classement
  (taille du programme, vitesse d'exécution).

## 4. Systèmes de RTS classiques

Clin d'œil assumé à Red Alert, Warcraft, StarCraft et Age of Empires :

- **Barre de construction latérale** (Red Alert) : on choisit un bâtiment, il se
  construit, puis on le pose.
- **Énergie** (Red Alert) : centrales et réseau électrique. En sous-production, les tours
  tirent moins vite et les radars s'éteignent. Couper l'énergie de l'ennemi devient une tactique.
- **Collecteurs** (Red Alert) : des véhicules qui récoltent le cristal en surface et le
  ramènent à la raffinerie, en complément des foreuses fixes.
- **Arbre technologique à prérequis** (Warcraft) : la forge nécessite la raffinerie,
  le laboratoire de fusion nécessite la forge, etc.
- **Héros** (Warcraft III) : le Forgeron-Ingénieur est une unité sur le terrain, qui gagne
  des niveaux et lance des pouvoirs liés aux gemmes (onde de gel, pluie de cristaux…).
- **Unités et armées** : sélection au cadre, groupes (Ctrl+1…9), ordres de patrouille,
  formations. Les armées complètent les tours pour aller détruire les nids.
- **Ravitaillement / population** (Warcraft, StarCraft) : chaque unité consomme du
  ravitaillement ; il faut construire des habitats.
- **Brouillard de guerre** : la carte se découvre en explorant ; radars et éclaireurs.
- **Factions** : les Forgerons (gemmes, tours), les Colons (industrie lourde, unités
  blindées) et, en escarmouche, la Corruption jouable (essaim, expansion organique).
- **Programmer son armée** : les ordres d'unité sont aussi accessibles par le code
  (`for soldat in groupe(1): soldat.patrouiller(a, b)`).

## 5. Système de gemmes

### 5.1 Couleurs (8)

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

### 5.2 Grade et fusion

- Deux gemmes de grade *n* → une gemme de grade *n+1* (coût exponentiel, puissance
  plus qu'exponentielle — comme GemCraft).
- Deux couleurs différentes → **gemme hybride** qui combine les effets (dilués).
- La fusion se fait dans des **Fours de fusion** : c'est une chaîne de production, pas
  un clic. Automatiser la fusion des hauts grades est un vrai défi d'ingénierie.

### 5.3 Où vont les gemmes

- **Tours** : la gemme sertie définit l'effet de la tour.
- **Pièges / pylônes** : effets de zone sur les chemins.
- **Bâtiments** : une gemme orange dans une foreuse = bonus de production, etc.
- **Bombes de gemmes** : on peut sacrifier une gemme pour une explosion massive.

## 6. Ennemis : la Corruption

- **Rampants** : nombreux, faibles, suivent les chemins.
- **Blindés** : lents, résistants (→ gemmes bleues).
- **Volants** : ignorent les murs, ciblent les convoyeurs.
- **Dévoreurs** : attaquent les bâtiments de production, pas les tours.
- **Nids** : la Corruption s'étend sur le terrain (à la Creeper World) ; il faut la
  repousser pour accéder à certains filons.
- **Boss de palier** : un par palier de forage, mécaniques uniques.

## 7. Apprentissage du code

### 7.1 Principe

- Les **golems-drones** et les **runes de contrôle** (posées sur des bâtiments)
  exécutent du code.
- Le code s'exécute **pas à pas, visiblement** : la ligne active est surlignée, on voit
  le drone agir, on peut mettre en pause, avancer d'une instruction, inspecter les
  variables.
- Chaque drone a un **budget d'instructions par seconde** (améliorable) : un code
  efficace est récompensé.
- Messages d'erreur **pédagogiques**, en français, avec indice.

### 7.2 Progression Python (Acte I)

1. Appels de fonctions (`miner()`, `aller_vers(cible)`)
2. Variables
3. Conditions (`if` / `else`)
4. Boucles (`while`, `for`)
5. Fonctions (`def`)
6. Listes et dictionnaires
7. Classes simples

### 7.3 Progression Rust (Acte II)

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

### 7.4 Exemple : le même golem, dans les deux langages

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

### 7.5 Langue du code : français ou anglais

Le joueur choisit dans les options la langue de l'**API du jeu** (les mots-clés de Python
et de Rust restent ceux des vrais langages) :

| Français                  | Anglais                  |
|---------------------------|--------------------------|
| `aller_vers(tour)`        | `move_to(tower)`         |
| `tour_la_plus_faible()`   | `weakest_tower()`        |
| `entrepot.prendre("jaune")` | `storage.take("yellow")` |
| `Couleur::Jaune`          | `Color::Yellow`          |

Les deux noms désignent la même fonction : un script écrit en français fonctionne pour
un joueur en anglais, et l'éditeur peut **traduire un script** d'une langue à l'autre
(utile pour passer ensuite à du vrai code, presque toujours écrit en anglais).
L'interface du jeu est elle aussi disponible en français et en anglais.

### 7.5 bis Le langage tel qu'implémenté (M3)

- Sous-ensemble de Python : entiers, booléens, textes, `None`, listes, tuples ; `if`/`elif`/
  `else`, `while`, `for … in`, `break`, `continue`, `def`/`return`, `global`, `pass` ;
  opérateurs `+ - * // % **`, comparaisons (enchaînables), `and`/`or`/`not` ;
  `len`, `range`, `abs`, `min`, `max`, `str`, `liste.append(x)`, indices négatifs, déballage
  `x, y = position()`.
- **Pas de nombres à virgule** (déterminisme) : `/` est refusé avec un message qui propose `//`.
- Les listes se **copient** à l'affectation (sémantique de valeur) : plus simple à expliquer,
  et l'état du jeu reste copiable pour le multijoueur. À signaler dans le cours.
- Compilation en bytecode, exécution par tranches (40 instructions par tick, ou 1 tous les 8
  ticks en mode pas à pas) : une boucle infinie ne bloque jamais le jeu.
- Les actions (`aller_vers`, `prendre`, `deposer`, `attendre`…) suspendent le programme jusqu'à
  ce qu'elles soient finies, comme dans 7 Billion Humans.
- L'arbre syntaxique est commun : le sous-ensemble Rust (Acte II) réutilisera compilateur et VM.

### 7.6 Mobile

Sur iPhone, l'éditeur propose un **mode blocs/cartes** (on assemble les instructions
au doigt) qui génère le même code texte. Sur ordinateur, éditeur texte complet avec
coloration et autocomplétion.

## 8. Multijoueur

- **Coopération** (2 à 4 joueurs) : une expédition partagée, chacun son QG ou une base commune.
- **Escarmouche / versus** : factions contre factions, ou une équipe contre la Corruption
  jouée par un autre joueur.
- **Réseau local (LAN)**, comme à l'époque : découverte automatique des parties sur le
  réseau, sans compte ni serveur. Clin d'œil aux LAN parties de Warcraft et Red Alert.
- **En ligne** : parties privées par code d'invitation.
- **Technique** : *lockstep* déterministe, comme les RTS classiques — seules les commandes
  des joueurs circulent sur le réseau, chaque machine calcule la même partie. C'est pourquoi
  la simulation est déterministe **dès le premier jour** (entiers, pas de flottants, hasard
  à graine ; vérifié par des tests sur Windows, Linux et macOS).
- **Limite du web** : un navigateur ne peut pas faire de découverte LAN. La version web
  rejoindra les parties en ligne (WebRTC / WebSocket) ; le LAN est réservé aux versions
  ordinateur.

## 9. Direction artistique (inspiration Dyson Sphere Program)

- **3D**, caméra orbitale libre (zoom de la vue d'ensemble jusqu'au niveau d'un golem).
- **Planète** : terrain avec **courbure d'horizon simulée par shader** (look « petite
  planète » de DSP, tout en gardant une grille de construction plate et simple).
  La courbure **dépend de l'angle de la caméra** : vue de dessus (pour construire), le
  monde est plat ; en inclinant la caméra vers l'horizon ou en dézoomant, la planète
  s'arrondit progressivement.
- **Lumière** : rendu HDR, **bloom** sur les gemmes et les machines, matériaux PBR
  (métal brossé, cristal, verre), ombres dynamiques, cycle jour/nuit.
- **Gemmes** : matériau émissif avec pseudo-réfraction, scintillement, halo coloré.
- **Mouvement** : milliers d'objets sur les convoyeurs (rendu instancié), bras
  robotiques animés, particules d'étincelles, traînées de projectiles.
- **Corruption** : matière noire organique et iridescente qui « respire ».
- **Ciel** : skybox avec la géante gazeuse / l'étoile du système, L'Arche visible en orbite.
- **Préréglages qualité** : Ultra (PC), Moyen (Mac portable), Mobile (iPhone).

## 10. Architecture technique

### 10.1 Choix

| Sujet           | Choix                                                            |
|-----------------|------------------------------------------------------------------|
| Langage         | Rust                                                             |
| Moteur          | Bevy (rendu wgpu : WebGPU / Vulkan / Metal / DX12, ECS)          |
| Langages du jeu | Interpréteur maison : 2 parseurs (sous-ensemble Python et Rust) → même bytecode → même VM |
| Simulation      | Tick fixe, **déterministe**, séparée du rendu (tests, replays)   |
| Sauvegardes     | Fichiers locaux (desktop) / stockage navigateur (web)            |

### 10.2 Organisation du code (workspace Cargo)

```
crates/
  gem_sim/      # Simulation pure et déterministe : carte, usine, gemmes, ennemis, vagues
  black_gem/    # Le jeu : rendu Bevy, caméra, interface (desktop + wasm)
  # À venir :
  gem_script/   # Lexer/parseurs Python & Rust, compilateur bytecode, VM sandboxée
  gem_net/      # Lockstep, LAN, en ligne
```

### 10.3 Plateformes « sans installation »

| Plateforme | Livraison                                                          |
|------------|--------------------------------------------------------------------|
| Windows    | Un `.exe` portable (aucun installeur)                              |
| Linux      | Un binaire portable (+ AppImage éventuelle)                        |
| macOS      | Une `.app` (signature Apple recommandée pour éviter l'alerte Gatekeeper) |
| Web        | WebAssembly + WebGPU (repli WebGL2), jouable dans le navigateur    |
| iPhone/iPad| Version web dans Safari, installable sur l'écran d'accueil (PWA)   |

## 11. Feuille de route

| Jalon | Contenu                                                                  |
|-------|--------------------------------------------------------------------------|
| **M0** ✅ | Squelette : workspace, planète 3D procédurale, courbure dynamique, caméra RTS, jour/nuit, FR/EN, builds desktop + web en CI |
| **M1** ✅ | Prototype jouable : foreuse → convoyeur → tour, 1 type d'ennemi, vagues nocturnes, victoire / défaite |
| **M2** ✅ | Gemmes : 5 couleurs de filons, taillerie, fours de fusion, hybrides, inventaire et sertissage, effets (dégâts, éclair, poison, ralentissement, multi-cibles) |
| **M3** ✅ | Langage `gem_script` (sous-ensemble Python, VM déterministe), golems-drones, éditeur avec pas à pas, modèles |
| **M4** ✅ | Formats d'expédition (Éclaireur, Expédition, Grand forage, Colonie), écran de départ, Cœur et Grand foreur, paliers avec boss, extraction et décollage |
| **M5** | L'Arche : méta-progression, recherche, bibliothèque de scripts          |
| **M6** | Passe graphique « DSP » : HDR, bloom, matériaux, particules, jour/nuit  |
| **M7** | Sous-ensemble Rust + Acte II (ownership comme mécanique)                |
| **M8** | Mobile : interface tactile, éditeur par blocs, préréglage qualité       |
| **M9** | Systèmes RTS : unités, héros, énergie, brouillard de guerre, factions   |
| **M10** | Multijoueur : coop et escarmouche, LAN puis en ligne                  |
| **M11** | Campagne scénarisée et briefings                                      |

## 12. Décisions prises

| Sujet | Décision |
|-------|----------|
| Langue de l'API du jeu | Au choix du joueur : français ou anglais (§7.5) |
| Durée des expéditions | De 15 min à plus d'une heure, plusieurs formats toujours disponibles (§3.2) |
| Multijoueur | Plus tard : coop et versus, en LAN et en ligne ; simulation déterministe dès M0 (§8) |
| Histoire | Les deux : campagne scénarisée et modes systémiques (§3.4) |
| Inspirations RTS | Red Alert, Warcraft, StarCraft, Age of Empires (§4) |
| Courbure de la planète | Dynamique selon l'angle et le zoom de la caméra (§9) |
| Graphisme | 3D façon Dyson Sphere Program |
