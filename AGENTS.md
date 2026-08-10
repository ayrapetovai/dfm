Read context.txt, README.md to learn the project.
After each feature implementation or fix update the context.txt, keep it concise.

## Coding style
- Make no banners.
- Name functions and variables with self-descriptive names.

## Tactics
- In the REVIEW_STRATEGY.md is described what source code review points must be considered.
- If any additional tools needed, do not install them, ask the user to install the tools for you.
- Do not install the built distributive of the project.
- Do not make any modifications in the dependent library sources.
- The project's binary dfm could be installed on the hosting system.
Don't execute it during tests.
Make sure that all XDG_* envs point to testing directory.
Otherwise dfm could damage user's files in home directory.

