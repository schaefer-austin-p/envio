# Fonts

All three families are under the SIL Open Font License 1.1; each folder
carries its `OFL.txt`. Only the weights the design uses are included. The
fonts are compiled into the app through `qml/qml.qrc` and also travel in the
click's `assets/` folder with their licences.

| Family | Version | Weights | Source |
| --- | --- | --- | --- |
| Inter | 4.1 | Regular, Medium, SemiBold | https://github.com/rsms/inter/releases/tag/v4.1 (`Inter-4.1.zip`, `extras/ttf/`) |
| IBM Plex Mono | 2.5.0 | Regular, Medium, SemiBold | https://github.com/IBM/plex/releases/tag/%40ibm%2Fplex-mono%402.5.0 (`ibm-plex-mono.zip`, `fonts/complete/ttf/`) |
| Lora | 3.021 | Regular, SemiBold | https://github.com/cyrealtype/Lora-Cyrillic/releases/tag/v3.021 (`Lora.zip`, `ttf/`) |

`Theme.qml` loads them; where one is missing it falls back to Ubuntu.
