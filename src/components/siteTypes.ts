export const TYPE_LABELS: Record<string, string> = {
  laravel:      "Laravel",
  wordpress:    "WordPress",
  codeigniter4: "CI4",
  codeigniter3: "CI3",
  spa:          "SPA",
  static:       "Static",
  generic:      "Generic",
};

// Avatar per project type: the framework's own color as a gradient, with
// white text, so it reads the same on both themes.
export const TYPE_AVATAR: Record<string, { bg: string; short: string }> = {
  laravel:      { bg: "linear-gradient(140deg, #FB7185, #E11D48)", short: "LV" },
  wordpress:    { bg: "linear-gradient(140deg, #38BDF8, #0369A1)", short: "WP" },
  codeigniter4: { bg: "linear-gradient(140deg, #FB923C, #DD4814)", short: "CI4" },
  codeigniter3: { bg: "linear-gradient(140deg, #F59E0B, #B45309)", short: "CI3" },
  spa:          { bg: "linear-gradient(140deg, #A78BFA, #6D28D9)", short: "SPA" },
  static:       { bg: "linear-gradient(140deg, #60A5FA, #4F46E5)", short: "</>" },
  generic:      { bg: "linear-gradient(140deg, #9CA3AF, #4B5563)", short: "PHP" },
};

export const avatarFor = (type: string) => TYPE_AVATAR[type] ?? TYPE_AVATAR.generic;
