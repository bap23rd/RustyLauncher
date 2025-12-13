import { useEffect, useState } from 'react';
import { getRustNews, type NewsItem } from '../services/steamApi';
import '../App.css';

export function HomePage() {
    const [news, setNews] = useState<NewsItem[]>([]);

    useEffect(() => {
        const fetchNews = async () => {
            const items = await getRustNews();
            setNews(items);
        };
        fetchNews();
    }, []);

    return (
        <section className="news-section">
            <h2>Latest Rust News</h2>
            <div className="news-list">
                {news.map((item) => (
                    <div key={item.gid} className="news-card">
                        <h3>{item.title}</h3>
                        <p className="news-date">{new Date(item.date * 1000).toLocaleDateString()}</p>
                        <div className="news-body">
                            {item.contents.replace(/\[.*?\]/g, '').substring(0, 200)}...
                        </div>
                        <a href={item.url} target="_blank" rel="noreferrer" className="read-more">Read Full Notes</a>
                    </div>
                ))}
            </div>
            <div className="news-footer">
                <a href="https://rust.facepunch.com/news/" target="_blank" rel="noreferrer" className="read-more-link">
                    Read more Rust news from Facepunch →
                </a>
            </div>
        </section>
    );
}
