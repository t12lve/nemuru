# Nemuru

Nemuru baisse le volume de Windows, puis met l'ordinateur en veille. On choisit la durée, la façon dont le son descend, et ce qui se passe à la fin.

Windows 10 ou 11, 64 bits. L'installeur est sur la [page des releases](https://github.com/t12lve/nemuru/releases/latest) : `Nemuru_Setup_v1.1.0.exe`.

La fenêtre s'ouvre sans rien lancer. Le compte à rebours commence seulement avec **Commencer**.

## Utilisation

1. Choisir une durée. Les raccourcis sont 15, 30, 45, 60 et 90 minutes. On peut aussi écrire une valeur entre 1 et 480.
2. Choisir la courbe de baisse, puis l'action de fin.
3. **Commencer**.

L'heure affichée sous le compte est le moment où l'action aura lieu. Une barre fine se remplit jusqu'à cette échéance.

Pendant le compte, on peut laisser la fenêtre ouverte ou la fermer : le délai continue, et l'icône reste dans la barre des tâches. Un clic dessus rouvre la fenêtre. Le menu propose **Ouvrir**, **Annuler** et **Quitter**.

**Échap** annule le compte en cours. **Annuler** et **Quitter** remettent le volume de départ. Une seconde fenêtre ne s'ouvre pas : un message prévient que Nemuru est déjà lancé.

Tant que le compte tourne, Windows ne s'endort pas de lui-même. Cet état d'éveil est relâché juste avant l'action finale.

La case **Ouvrir au démarrage de Windows** ajoute Nemuru au démarrage de la session. Au login, seule la fenêtre s'ouvre. Le compte ne démarre pas tout seul.

## Le volume

Nemuru agit sur le volume principal, celui du curseur de la barre des tâches. Il mémorise le niveau et l'état muet, puis les remet dans les cas décrits plus bas.

| Courbe | Ce qu'on entend |
| --- | --- |
| À l'oreille | La baisse semble régulière. Le niveau descend de façon linéaire en décibels, de 0 dB à −60 dB. C'est le réglage de départ. |
| Linéaire | Le pourcentage descend d'un pas égal chaque minute. |
| En douceur | La baisse commence lentement, s'accélère, puis se termine lentement. |
| Sur la fin | Le volume tient, puis descend pendant les dernières minutes. Le champ **Pendant** indique combien. |

## À la fin

| Action | Ensuite |
| --- | --- |
| Veille | Le volume revient au réveil. |
| Hibernation | Le volume revient à la sortie de veille. |
| Écran éteint | Le son reste coupé. **Rétablir le son** le remet, de même que quitter l'application. |
| Arrêt | L'ordinateur s'éteint. Le volume n'est pas remis avant l'arrêt. |

## Google TV

L'extinction de la télé est décochée. Il faut une adresse sur le même réseau, ou **Chercher**, qui interroge le réseau en mDNS.

La commande envoyée est `STANDBY` du protocole Cast, en TLS, sur le port 8009. Nemuru n'annonce que la télé a accepté que si une réponse revient. Si la télé ne répond pas, l'action choisie pour l'ordinateur a quand même lieu.

## Fichiers

Les réglages sont dans `%APPDATA%\Nemuru\config.json`. Le journal est à côté : `nemuru.log`.

Un ancien fichier `%APPDATA%\SommeilDouceur\config.json` est repris au premier lancement.

## Compilation

Rust stable :

```
cargo build --release
```

L'exécutable est `target\release\nemuru.exe`.

L'installeur se construit avec [Inno Setup 6](https://jrsoftware.org/isinfo.php) :

```
build.bat
```

ou, si le compilateur est déjà dans le chemin habituel :

```
"C:\Program Files (x86)\Inno Setup 6\ISCC.exe" installer\installer.iss
```

Le fichier produit est `dist-installer\Nemuru_Setup_v1.1.0.exe`.

Les tests vérifient les courbes de volume. Ils ne touchent ni au son ni à la veille :

```
cargo test
```

## Licence

MIT. Le texte est dans `LICENSE.txt`.
