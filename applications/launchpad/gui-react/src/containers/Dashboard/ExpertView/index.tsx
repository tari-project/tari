import { useAppDispatch, useAppSelector } from '../../../store/hooks'
import { setExpertView } from '../../../store/app'
import { selectExpertView } from '../../../store/app/selectors'
import { selectState } from '../../../store/services/selectors'

const ExpertView = () => {
  const dispatch = useAppDispatch()
  const expertView = useAppSelector(selectExpertView)
  const servicesState = useAppSelector(selectState)

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
      <pre style={{ color: 'white' }}>
        {JSON.stringify(servicesState, null, 2)}
      </pre>
    </div>
  )
}

export default ExpertView
