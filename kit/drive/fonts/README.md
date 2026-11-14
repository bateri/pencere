The faces a picture is drawn in where the machine has none of its own (`shot::Fonts::carried`, and
`Fonts::installed` for a family it finds no face of): JetBrains Mono for the terminal's family and
Inter for the system's, unmodified copies of the upstream variable fonts (upright and italic; the
`wght` axis covers every weight a page asks for, Inter's `opsz` the size of the text), each with its
own license text.

| files                                                | what                               | source                                                                                                                                            | sha256                                                           |
| ---------------------------------------------------- | ---------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------- | ---------------------------------------------------------------- |
| `JetBrainsMono[wght].ttf`                            | JetBrains Mono 2.305, upright      | JetBrains/JetBrainsMono, `fonts/variable/` at commit 19371302b95d. https://github.com/JetBrains/JetBrainsMono                                     | 3cfafa86e28b87184d592fef82846e8c10cb48653c62efcda34f082da225ec34 |
| `JetBrainsMono-Italic[wght].ttf`                     | JetBrains Mono 2.305, italic       | same                                                                                                                                              | c5c2a1d29297d2a40ec4a0114447c6b7a1477678ab2d9aa4b755356e1a02cead |
| `JetBrainsMono-OFL.txt`, `JetBrainsMono-AUTHORS.txt` | SIL Open Font License 1.1, authors | the same commit; the v2.304 release                                                                                                               |                                                                  |
| `InterVariable.ttf`                                  | Inter 4.1, upright                 | rsms/inter release v4.1 (`Inter-4.1.zip`, sha256 9883fdd4a49d4fb66bd8177ba6625ef9a64aa45899767dde3d36aa425756b11e). https://github.com/rsms/inter | 4989b125924991b90d05b2d16e0e388c48f7d5bb8b30539bbf9c755278d0ccaf |
| `InterVariable-Italic.ttf`                           | Inter 4.1, italic                  | same                                                                                                                                              | d6f1f6a172d9e588438db9f986fd5cfad7b30f644374080a8a9d4d91e344586f |
| `Inter-LICENSE.txt`                                  | SIL Open Font License 1.1          | the same archive (`LICENSE.txt`)                                                                                                                  | 262481e844521b326f5ecd053e59b98c8b2da78c8ee1bdbb6e8174305e54935a |

The crate keeps them as they are upstream; `build.rs` packs them with zlib into the build's folder, and
the program carries them packed (`include_bytes!`, linked only with the feature `shot`) and unpacks a
face once, where it first draws in it, which a machine with faces of its own never does.
