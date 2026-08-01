import { createContext, useContext } from 'react';

export type ThemeMode = 'light' | 'dark';

interface ThemeModeCtx {
  mode: ThemeMode;
  toggle: () => void;
}

export const ThemeModeContext = createContext<ThemeModeCtx>({ mode: 'light', toggle: () => {} });

export const useThemeMode = () => useContext(ThemeModeContext);
