import { useEffect, useState } from 'react';
import type { CuratedVersion } from '../types/curatedVersions';
import '../App.css';

interface DownloadPageProps {
    onOpenCuratedDownload: (version: CuratedVersion) => void;
}

export function DownloadPage({ onOpenCuratedDownload }: DownloadPageProps) {
    const [recommendedVersions, setRecommendedVersions] = useState<CuratedVersion[]>([]);
    const [otherVersions, setOtherVersions] = useState<CuratedVersion[]>([]);

    useEffect(() => {
        loadCuratedVersions();
    }, []);

    const loadCuratedVersions = async () => {
        try {
            const rlData = await window.electronAPI.getCuratedVersionsRL();
            const otherData = await window.electronAPI.getCuratedVersionsOther();
            // versionRL.json uses "recommended" key
            setRecommendedVersions(rlData?.recommended || []);
            // versionOther.json uses "other" key
            setOtherVersions(otherData?.other || []);
        } catch (error) {
            console.error('Error loading curated versions:', error);
        }
    };

    return (
        <>
            {/* RustyLauncher Recommended Section */}
            <section className="news-section">
                <h2>RustyLauncher Recommended</h2>
                <div className="news-list">
                    {recommendedVersions.length === 0 ? (
                        <p className="empty-message">No recommended versions available.</p>
                    ) : (
                        recommendedVersions.map((version, index) => (
                            <div key={index} className="news-card">
                                <h3>{version.title}</h3>
                                <p className="news-date">{version.versionDate}</p>
                                <button
                                    className="read-more"
                                    onClick={() => onOpenCuratedDownload(version)}
                                >
                                    Download
                                </button>
                            </div>
                        ))
                    )}
                </div>
            </section>

            <hr className="section-divider" />

            {/* Other Versions Section */}
            <section className="news-section">
                <h2>Other Versions</h2>
                <div className="news-list">
                    {otherVersions.length === 0 ? (
                        <p className="empty-message">No other versions available.</p>
                    ) : (
                        otherVersions.map((version, index) => (
                            <div key={index} className="news-card">
                                <h3>{version.title}</h3>
                                <p className="news-date">{version.versionDate}</p>
                                <button
                                    className="read-more"
                                    onClick={() => onOpenCuratedDownload(version)}
                                >
                                    Download
                                </button>
                            </div>
                        ))
                    )}
                </div>
            </section>
        </>
    );
}
