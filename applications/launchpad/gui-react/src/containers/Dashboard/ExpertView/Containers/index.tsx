import Containers from './Containers'

const ContainersContainer = () => {
  const services = [
    {
      id: 'asdflksajdflkasjdf',
      name: 'Tor',
      cpu: 1.2,
      memory: '8 MB',
      running: true,
    },
    {
      id: 'oiausdofiasdofiu',
      name: 'Base Node',
      cpu: 113,
      memory: '12 MB',
      running: false,
    },
    {
      id: 'oiauweroasidfu',
      name: 'Wallet',
      cpu: 3,
      memory: '4.4 GB',
      running: false,
      pending: true,
    },
    {
      id: 'oiauweroasidfu',
      name: 'SHA3 miner',
      cpu: 11,
      memory: '1012 MB',
      running: true,
      pending: true,
    },
  ]

  return <Containers services={services} />
}

export default ContainersContainer
