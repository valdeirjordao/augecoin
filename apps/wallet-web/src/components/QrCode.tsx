import { useEffect, useState } from 'react';

export function QrCode({ data }: { data: string }) {
  const [dataUrl, setDataUrl] = useState('');
  useEffect(() => {
    let active = true;
    import('qrcode')
      .then((QRCode) => QRCode.toDataURL(data, { width: 200, margin: 2 }))
      .then((url) => {
        if (active) setDataUrl(url);
      })
      .catch(() => {
        if (active) setDataUrl('');
      });
    return () => {
      active = false;
    };
  }, [data]);
  if (!dataUrl) return <div className="qr-placeholder">Carregando QR…</div>;
  return <img src={dataUrl} alt="QR Code" data-testid="qr-code-img" />;
}
