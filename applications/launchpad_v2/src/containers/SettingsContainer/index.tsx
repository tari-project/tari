import Modal from '../../components/Modal'

const SettingsContainer = ({
  open,
  onClose,
}: {
  open?: boolean
  onClose: () => void
}) => {
  return (
    <Modal open={open} onClose={onClose}>
      <button onClick={onClose}>close</button>
      <p>hello world</p>
    </Modal>
  )
}

export default SettingsContainer
