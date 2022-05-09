import * as React from 'react'
import { SVGProps } from 'react'

const SvgMoneroSignet = (props: SVGProps<SVGSVGElement>) => (
  <svg
    xmlns='http://www.w3.org/2000/svg'
    width='1em'
    height='1em'
    viewBox='0 0 79 79'
    fill='none'
    data-testid='svg-monerosignet'
    {...props}
  >
    <path
      fillRule='evenodd'
      clipRule='evenodd'
      d='M63.1559 59.6334C57.4583 66.3214 48.9745 70.5631 39.5 70.5631C30.0255 70.5631 21.5417 66.3214 15.8441 59.6334H23.585V36.4916L39.5 55.2856L55.4151 36.4916V59.6334H63.1559ZM68.03 51.8063C69.66 48.0328 70.5631 43.8719 70.5631 39.5C70.5631 31.822 67.7775 24.7946 63.1617 19.3734V51.8063H68.03ZM61.3605 17.4312L39.5 43.2461L17.6395 17.4312C23.2523 11.8709 30.9751 8.43689 39.5 8.43689C48.0249 8.43689 55.7477 11.8709 61.3605 17.4312ZM15.8384 19.3734C11.2226 24.7946 8.43689 31.822 8.43689 39.5C8.43689 43.8719 9.34005 48.0328 10.97 51.8063H15.8384V19.3734ZM79 39.5C79 61.3153 61.3153 79 39.5 79C17.6848 79 0 61.3153 0 39.5C0 17.6848 17.6848 0 39.5 0C61.3153 0 79 17.6848 79 39.5Z'
      fill='currentColor'
    />
  </svg>
)

export default SvgMoneroSignet
