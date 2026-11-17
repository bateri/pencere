// Code is coloured by Shiki into variables (--code-*), and the site's own tones give them colour, in
// a light theme and a dark one alike.
import { createCssVariablesTheme } from "shiki";
import { install } from "./project.js";

export const theme = createCssVariablesTheme({
  name: "pencere",
  variablePrefix: "--code-",
  variableDefaults: {},
});

export const REPO = "https://github.com/bateri/pencere";
// The line that installs the command, as the README has it.
export const INSTALL = install().command;
