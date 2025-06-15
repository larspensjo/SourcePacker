# Instructions used by AI agents

* When fixing bugs, consider whether a unit test could have caught the issue, and if so, write that test first.
* Minimize the size of changes. Avoid large-scale refactoring or redesign unless explicitly instructed.
* Add a concise block comment (up to 2 short paragraphs) for each new class, module, or namespace explaining its purpose.
* Add similar 2-paragraph block comments for new key or complex functions.
* Use block comments (`/* ... */`) for multi-line comments and avoid XML-style documentation comments. But one-line comments can use regular // or whatever is best practice.
* Keep relevant comments and update them if needed. Remove outdated comments about prior changes.
* Add unit tests where reasonable, especially for bug fixes or new logic.
* Use Dependency Injection and mock objects to facilitate testing, when applicable.
* Include simple, clear debug log messages to help trace behavior, but avoid logging inside high-frequency code paths to reduce noise.
* At the end of a complete task, suggest potential next tasks if relevant and not already covered by the current development plan.
* Follow idiomatic patterns and language standards for the given programming language.
* When creating unit tests, use sections "Arrange", "Act", and "Assert". Add a single-line comment for each section start.
* Prefer the use of early-out. That is, test for the failure case, handle the error and return, and then the main logic. The goal is to reduce the indentation level of the main logic.
* Don't remove comments with a TODO in them.
* For structs and records, use private declaration of members by default.
* Ensure that all locked regions are kept to a minimum, using clear block delimiters. This is good practice.
* If you make changes to code that depends on windows-rs, make sure to look up the latest API from internet, version 0.61.1.

# This is needed for formatting

rustup component add rustfmt

# Clippy is needed for linting

rustup component add clippy
