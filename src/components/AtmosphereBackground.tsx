import './AtmosphereBackground.css';

interface AtmosphereBackgroundProps {
    /** Paths (relative to /public) of background images to fade between. Leave empty for just the dark overlay. */
    images?: string[];
}

/**
 * Slow-crossfading background imagery with a dark overlay, used to give full-bleed
 * pages (homepage, etc.) some Rust atmosphere without the images fighting the foreground content.
 * Drop images into /public/backgrounds and pass their paths in to activate the slideshow.
 */
export function AtmosphereBackground({ images = [] }: AtmosphereBackgroundProps) {
    return (
        <div className="atmosphere-background">
            {images.map((src, i) => (
                <div
                    key={src}
                    className="atmosphere-layer"
                    style={{
                        backgroundImage: `url(${src})`,
                        animationDelay: `${i * (24 / images.length)}s`,
                    }}
                />
            ))}
            <div className="atmosphere-overlay" />
        </div>
    );
}
