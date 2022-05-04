import { useDispatch, useSelector } from 'react-redux'
import { setExpertView } from '../../../store/app'
import { selectExpertView } from '../../../store/app/selectors'

const ExpertView = () => {
  const dispatch = useDispatch()
  const expertView = useSelector(selectExpertView)

  return (
    <div>
      <p style={{ color: '#fff' }}>Expert View</p>
      <button
        onClick={() =>
          dispatch(
            setExpertView(expertView === 'fullscreen' ? 'open' : 'fullscreen'),
          )
        }
      >
        Fullscreen
      </button>
    </div>
  )
}

export default ExpertView
