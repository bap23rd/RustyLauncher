import { useEffect, useState } from 'react';
import { Star, Stack, DownloadSimple } from '@phosphor-icons/react';
import type { CuratedVersion } from '../types/curatedVersions';
import './DownloadPage.css';

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
        <div className="curated-sections">
            <section className="curated-section">
                <h2><Star size={24} weight="bold" /> RustyLauncher Recommended</h2>
                {recommendedVersions.length === 0 ? (
                    <p className="empty-message">No recommended versions available.</p>
                ) : (
                    <div className="curated-grid">
                        {recommendedVersions.map((version, index) => (
                            <div key={index} className="curated-card">
                                <h3>{version.title}</h3>
                                <p className="version-date">{version.versionDate}</p>
                                <button
                                    className="btn-download"
                                    onClick={() => onOpenCuratedDownload(version)}
                                >
                                    <DownloadSimple size={16} weight="bold" /> Download
                                </button>
                            </div>
                        ))}
                    </div>
                )}
            </section>

            <hr className="section-divider" />

            <section className="curated-section">
                <h2><Stack size={24} weight="bold" /> Other Versions</h2>
                {otherVersions.length === 0 ? (
                    <p className="empty-message">No other versions available.</p>
                ) : (
                    <div className="curated-grid">
                        {otherVersions.map((version, index) => (
                            <div key={index} className="curated-card">
                                <h3>{version.title}</h3>
                                <p className="version-date">{version.versionDate}</p>
                                <button
                                    className="btn-download"
                                    onClick={() => onOpenCuratedDownload(version)}
                                >
                                    <DownloadSimple size={16} weight="bold" /> Download
                                </button>
                            </div>
                        ))}
                    </div>
                )}
            </section>
        </div>
    );
}
