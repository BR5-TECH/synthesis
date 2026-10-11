import mark32 from "../../src-tauri/icons/synthesis-32x32.png";
import mark64 from "../../src-tauri/icons/synthesis-64x64.png";

/**
 * SNV-FR-EZQB / PPK-FR-DYKZ: the application mark of the app brand. The mark
 * has a transparent background, so it reads on the light and the dark theme.
 * The browser picks the 32 px or the 64 px file for the rendered size and the
 * display density. The mark is decorative: the name "Synthesis" beside it
 * gives the brand its accessible text.
 */
export function AppIcon({ size }: { size: number }) {
  return (
    <img
      className="app-icon"
      src={mark32}
      srcSet={`${mark32} 32w, ${mark64} 64w`}
      sizes={`${size}px`}
      alt=""
      width={size}
      height={size}
      draggable={false}
      data-testid="app-icon"
    />
  );
}
