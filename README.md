# Nemuru

L'application baisse le volume de Windows, puis exécute l'action choisie : veille, hibernation, extinction de l'écran ou arrêt. Elle peut aussi demander à une Google TV du réseau de s'éteindre.

La fenêtre s'ouvre sans rien lancer. Le compte à rebours commence seulement quand on appuie sur Commencer. L'annulation, la fermeture et le réveil après une veille remettent le volume de départ.

## Utilisation

1. Choisir la durée, la courbe et ce qui se passe à la fin.
2. Commencer.
3. Laisser la fenêtre ouverte, ou la fermer : le compte continue, l'icône reste dans la barre des tâches.

Échap annule. Quitter depuis la barre des tâches remet le volume.

La courbe « À l'oreille » descend de façon régulière pour l'oreille (linéaire en décibels, de 0 dB à −60 dB). « Sur la fin » garde le volume, puis le baisse pendant les dernières minutes indiquées.

Pendant le compte, Windows ne s'endort pas de lui-même. L'état d'éveil est relâché juste avant l'action finale.

Si l'action est « Écran éteint », le son reste coupé tant que la fenêtre est ouverte. « Rétablir le son » le remet. Fermer l'application le remet aussi.

## Google TV

L'extinction est décochée par défaut. Il faut l'adresse de la télé, sur le même réseau. « Chercher » interroge le réseau en mDNS.

La commande envoyée est `STANDBY` du protocole Cast (port 8009, TLS). L'application ne dit que la télé a accepté que si une réponse revient. Sinon la veille de l'ordinateur a quand même lieu.

## Démarrage de Windows

La case « Ouvrir au démarrage de Windows » ajoute Nemuru à la clé Run de l'utilisateur. Au login, seule la fenêtre s'ouvre.

## Configuration

`%APPDATA%\Nemuru\config.json`

Le journal est à côté : `nemuru.log`. Un ancien fichier `%APPDATA%\SommeilDouceur\config.json` est repris au premier lancement.

## Compilation

Rust stable, puis :

```
cargo build --release
```

L'exécutable est `target\release\nemuru.exe`.

Pour l'installeur, Inno Setup 6, puis `build.bat` ou :

```
"C:\Program Files (x86)\Inno Setup 6\ISCC.exe" installer\installer.iss
```

Le fichier produit est `dist-installer\Nemuru_Setup_v1.1.0.exe`.
