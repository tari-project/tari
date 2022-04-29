import { SVGProps } from 'react'

const Smiley = ({
  on,
  ...props
}: { on: boolean } & SVGProps<SVGSVGElement>) => {
  if (on) {
    return (
      <svg
        width='20'
        height='20'
        viewBox='0 0 20 20'
        fill='none'
        xmlns='http://www.w3.org/2000/svg'
        {...props}
      >
        <path
          d='M1.35288 6.95043C2.00437 4.17301 4.17301 2.00437 6.95043 1.35288C8.95626 0.882374 11.0437 0.882375 13.0496 1.35288C15.827 2.00437 17.9956 4.17301 18.6471 6.95044C19.1176 8.95626 19.1176 11.0437 18.6471 13.0496C17.9956 15.827 15.827 17.9956 13.0496 18.6471C11.0437 19.1176 8.95626 19.1176 6.95044 18.6471C4.17301 17.9956 2.00437 15.827 1.35288 13.0496C0.882374 11.0437 0.882374 8.95626 1.35288 6.95043Z'
          stroke='currentColor'
          strokeWidth='1.5'
        />
        <path
          d='M4 9C4.29941 8.4022 4.85904 8 5.5 8C6.14096 8 6.70059 8.4022 7 9'
          stroke='currentColor'
          strokeWidth='1.5'
          strokeLinecap='round'
          strokeLinejoin='round'
        />
        <path
          d='M14 13C13.2016 14.1956 11.7092 15 10 15C8.29077 15 6.79844 14.1956 6 13'
          stroke='currentColor'
          strokeWidth='1.5'
          strokeLinecap='round'
          strokeLinejoin='round'
        />
        <path
          d='M13 9C13.2994 8.4022 13.859 8 14.5 8C15.141 8 15.7006 8.4022 16 9'
          stroke='currentColor'
          strokeWidth='1.5'
          strokeLinecap='round'
          strokeLinejoin='round'
        />
      </svg>
    )
  }

  return (
    <svg
      width='21'
      height='21'
      viewBox='0 0 21 21'
      fill='none'
      xmlns='http://www.w3.org/2000/svg'
      {...props}
    >
      <path
        d='M4.75 17.875C5.51526 18.4077 6.38543 18.802 7.32533 19.0224C9.33115 19.4929 11.4186 19.4929 13.4245 19.0224C16.2019 18.3709 18.3705 16.2023 19.022 13.4249C19.4925 11.419 19.4925 9.33155 19.022 7.32573C18.8134 6.43648 18.4493 5.60963 17.9595 4.875'
        stroke='currentColor'
        strokeWidth='1.5'
        strokeLinecap='round'
      />
      <path
        d='M14.375 13.375C13.5766 14.5706 12.0842 15.375 10.375 15.375C9.41415 15.375 8.52183 15.1208 7.78281 14.6858'
        stroke='currentColor'
        strokeWidth='1.5'
        strokeLinecap='round'
        strokeLinejoin='round'
      />
      <path
        d='M13.375 9.375C13.6744 8.7772 14.234 8.375 14.875 8.375C15.516 8.375 16.0756 8.7772 16.375 9.375'
        stroke='currentColor'
        strokeWidth='1.5'
        strokeLinecap='round'
        strokeLinejoin='round'
      />
      <path
        d='M4.375 9.375C4.67441 8.7772 5.23404 8.375 5.875 8.375C6.51596 8.375 7.07559 8.7772 7.375 9.375'
        stroke='currentColor'
        strokeWidth='1.5'
        strokeLinecap='round'
        strokeLinejoin='round'
      />
      <path
        fillRule='evenodd'
        clipRule='evenodd'
        d='M1.86744 15.8077C1.47806 15.1263 1.18242 14.3833 0.997697 13.5959C0.500768 11.4774 0.500768 9.27263 0.997697 7.15415C1.71424 4.09941 4.09941 1.71424 7.15416 0.997695C9.27264 0.500768 11.4774 0.500768 13.5958 0.997696C14.3896 1.18389 15.1382 1.48275 15.824 1.87675L14.7148 2.9839C14.2554 2.75592 13.7658 2.57829 13.2533 2.45806C11.3601 2.01398 9.38988 2.01398 7.49671 2.45806C4.99661 3.0445 3.0445 4.9966 2.45806 7.49671C2.01398 9.38988 2.01398 11.3601 2.45806 13.2533C2.57703 13.7605 2.7522 14.2452 2.97676 14.7004L1.86744 15.8077Z'
        fill='currentColor'
      />
      <path
        d='M17.375 1.375L1.375 17.375'
        stroke='currentColor'
        strokeWidth='1.5'
        strokeLinecap='round'
      />
    </svg>
  )
}

export default Smiley
