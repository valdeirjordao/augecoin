import { useEffect, useState } from 'react';
import { useAccount } from '../hooks/useAccount';

function QrCode({ data }: { data: string }) {
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

export function Receive() {
  const { owned } = useAccount();
  const [selected, setSelected] = useState<number | ''>('');

  const account = owned.find((a) => a.account_number === selected);
  const payload = account ? `augeid:${account.account_number}` : '';

  return (
    <div className="page">
      <div className="page-head">
        <h2>Receber AUGE</h2>
        <p className="muted">Compartilhe seu número de AUGEID ou o QR code.</p>
      </div>

      <div className="card">
        <label className="field">
          <span className="field-label">AUGEID</span>
          <select value={selected} onChange={(e) => setSelected(Number(e.target.value))} className="text-input" data-testid="receive-select">
            <option value="">Selecione…</option>
            {owned.map((a) => (
              <option key={a.account_number} value={a.account_number}>
                AUGEID #{a.account_number} {a.name ? `(${a.name})` : ''}
              </option>
            ))}
          </select>
        </label>

        {account && (
          <>
            <div className="receive-info">
              <div>
                <span className="field-label">Número do AUGEID</span>
                <div className="receive-number" data-testid="receive-number">
                  {account.account_number}
                </div>
              </div>
              <div>
                <span className="field-label">Nome</span>
                <div className="receive-name" data-testid="receive-name">
                  {account.name ?? 'Sem nome'}
                </div>
              </div>
            </div>
            <div className="qr-container">
              <QrCode data={payload} />
            </div>
            <code data-testid="receive-payload">{payload}</code>
          </>
        )}
      </div>
    </div>
  );
}
