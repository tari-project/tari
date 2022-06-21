const Logs = () => {
  return (
    <iframe
      id='grafanaIframe'
      title='Your Grafana'
      frameBorder={0}
      style={{ overflow: 'auto', width: '100%', height: '100%' }}
      src='http://localhost:18300'
    ></iframe>
  )
}

export default Logs
