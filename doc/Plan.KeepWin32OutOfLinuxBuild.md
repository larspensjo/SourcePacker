# Refactor Plan — Keep Win32 Code Out of the Linux Build Graph (SourcePacker)

**Goal:** Let GitHub CI on Linux compile and run all unit tests that don’t depend on Win32 by ensuring only platform-agnostic code is built on non-Windows, while Windows builds retain full functionality.

---

## Pre-flight checklist

* Confirm you can build and test on Windows locally.
* Ensure CI runs on both Windows and Linux.
* Ensure the `windows` crate (or any Win32 bindings) is declared only for Windows targets in your dependency config.

---

## Target module layout (conceptual)

* A platform folder that already contains your platform-agnostic “types”.
* A new file that contains only portable styling **primitives** (color, font description/weight, control style, style ID) with no Win32 references.
* The existing Windows styling implementation continues to exist and will **re-export** the primitives while adding Win32 glue.
* On non-Windows, a **stub styling module** re-exports only the portable primitives (no Win32).

Public names for styling remain identical across platforms so your app-logic imports don’t change.

---

## Step-by-step

1. **Create a new “styling primitives” module**

   * Move the portable styling types (color, font description/weight, control style, style ID) from the Windows styling file into a new module.
   * Remove all Win32, FFI, or OS-specific imports from these definitions.
   * Verify this module compiles on any platform by itself.

2. **Keep Windows-only implementation separate**

   * Leave all Win32 integrations (e.g., handle conversions, GDI/DirectWrite glue, OS calls, drops) in the existing Windows styling module.
   * At the top of that Windows module, expose the same public types by **re-exporting** the primitives from the new module.
   * Keep any Windows-only helper structs or functions here.

3. **Ensure platform-agnostic types continue to import styling**

   * Your platform “types” module should keep referring to styling through a sibling module path.
   * Do not change app-logic imports; aim to keep the same publicly available names from the platform layer.

4. **Provide a non-Windows platform stub**

   * In your platform wiring for non-Windows builds, reference the platform “types” file and the new “styling primitives” file.
   * Define a **stub styling module** that only re-exports the primitives, so the publicly visible names match Windows, but without any Win32 code.
   * Re-export from this stub exactly the identifiers your app logic expects (color, control style, font description/weight, style ID, plus the items from the platform “types” module).

5. **Verify Windows wiring**

   * On Windows builds, ensure the platform module includes both the platform “types” and the real Windows styling implementation.
   * Re-export the same set of public names as on non-Windows so app-logic imports remain identical.

6. **Strengthen conditional compilation**

   * Make sure Win32 dependencies are declared only for Windows in your dependency configuration.
   * Add conditional compilation attributes where needed in Windows-only files so they never enter the non-Windows build graph.

7. **Sanity scan for accidental Win32 leakage**

   * Search the new primitives module to confirm there are no references to Windows types, handles, or imports.
   * Confirm the primitives module has no conditional compilation attributes that would tie it to Windows.

8. **Local build & test (Windows)**

   * Run check, tests, and lints with all targets enabled.
   * Confirm no regressions in functional behavior.

9. **Local build & test (Linux or a Linux container)**

   * Run check, tests, and lints with all targets enabled.
   * Confirm the build succeeds and tests that do not depend on Win32 run.

10. **Verify dependency graph on Linux**

* Inspect the resolved dependency tree on Linux and confirm the Windows bindings crate does not appear.

11. **CI matrix configuration**

* Ensure the GitHub Actions matrix includes both Windows and Linux.
* Run “check”, “test”, and “clippy” on both.
* Fail on warnings to keep the surface tidy.

12. **Commit plan (small, safe increments)**

* First commit: add the new primitives module with moved types (no behavior changes).
* Second: make the Windows styling module re-export primitives and keep Win32 glue.
* Third: add/adjust the non-Windows stub wiring to expose identical public names.
* Fourth: enforce conditional compilation and target-specific dependencies.
* Fifth: CI updates and final green builds on both platforms.

---

## Optional: increase test reach with an abstraction trait

* Define a platform abstraction trait that your Windows platform layer implements.
* Provide a lightweight mock implementation compiled on non-Windows (for tests only).
* Keep the trait in a portable module so Linux CI can unit-test more app-logic paths without Win32.

---

## Rollback plan

* Changes are additive and mostly wiring. Roll back in reverse commit order.
* If necessary, move the primitives back into the original styling file and remove the non-Windows stub wiring.

---

## Acceptance criteria

* Linux CI builds and runs all tests that do not depend on Win32.
* The Windows bindings crate is absent from the Linux dependency tree.
* Windows builds behave exactly as before with full styling and Win32 functionality.
